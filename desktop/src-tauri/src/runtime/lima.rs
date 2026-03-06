use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use super::types::{ContainerStatus, RuntimeStatus, VmStatus};
use super::ContainerRuntime;
use crate::error::TerrariumError;

const DEV_IMAGE: &str = "terrarium/dev-base:latest";

const VM_NAME: &str = "terrarium";

pub struct LimaRuntime {
    limactl_path: Option<PathBuf>,
}

impl LimaRuntime {
    pub fn new() -> Self {
        let limactl_path = Self::find_limactl();
        Self {
            limactl_path,
        }
    }

    /// Find the limactl binary. Checks PATH first, then common Homebrew locations.
    fn find_limactl() -> Option<PathBuf> {
        let candidates = [
            "/opt/homebrew/bin/limactl",
            "/usr/local/bin/limactl",
        ];

        // Check common locations first (GUI apps have minimal PATH)
        for path in &candidates {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // Fall back to PATH lookup
        which::which("limactl").ok()
    }

    fn limactl(&self) -> Result<Command, TerrariumError> {
        match &self.limactl_path {
            Some(path) => Ok(Command::new(path)),
            None => Err(TerrariumError::LimaNotInstalled),
        }
    }

    fn container_name(project_id: &str) -> String {
        format!("terrarium-{}-dev", project_id)
    }

    fn volume_name(project_id: &str) -> String {
        format!("terrarium-{}-claude-config", project_id)
    }
}

#[async_trait]
impl ContainerRuntime for LimaRuntime {
    async fn check_prerequisites(&self) -> Result<(), TerrariumError> {
        let _ = self.limactl()?;
        Ok(())
    }

    async fn vm_status(&self) -> VmStatus {
        if self.limactl_path.is_none() {
            return VmStatus::NotInstalled;
        }

        let mut cmd = match self.limactl() {
            Ok(cmd) => cmd,
            Err(_) => return VmStatus::NotInstalled,
        };

        let future = cmd.args(["list", "--json"]).output();
        let output = match tokio::time::timeout(std::time::Duration::from_secs(10), future).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return VmStatus::Error {
                    message: e.to_string(),
                }
            }
            Err(_) => {
                return VmStatus::Error {
                    message: "limactl list timed out after 10s".into(),
                }
            }
        };

        if !output.status.success() {
            return VmStatus::Error {
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            };
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // limactl list --json outputs one JSON object per line (NDJSON)
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(vm) = serde_json::from_str::<serde_json::Value>(line) {
                if vm.get("name").and_then(|n| n.as_str()) == Some(VM_NAME) {
                    return match vm.get("status").and_then(|s| s.as_str()) {
                        Some("Running") => VmStatus::Running,
                        Some("Stopped") => VmStatus::Stopped,
                        Some(other) => VmStatus::Error {
                            message: format!("Unknown VM status: {}", other),
                        },
                        None => VmStatus::Error {
                            message: "No status field in VM info".into(),
                        },
                    };
                }
            }
        }

        VmStatus::NotCreated
    }

    async fn runtime_status(&self) -> RuntimeStatus {
        let vm_status = self.vm_status().await;

        let lima_version = if self.limactl_path.is_some() {
            if let Ok(mut cmd) = self.limactl() {
                if let Ok(output) = cmd.args(["--version"]).output().await {
                    if output.status.success() {
                        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        Some(version)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        RuntimeStatus {
            vm_status,
            lima_version,
        }
    }

    async fn ensure_vm_ready(&self) -> Result<(), TerrariumError> {
        let status = self.vm_status().await;

        match status {
            VmStatus::Running => Ok(()),
            VmStatus::Stopped => self.start_vm().await,
            VmStatus::NotCreated => {
                // Create VM from template
                let yaml_path = self.find_vm_template()?;

                let mut cmd = self.limactl()?;
                let future = cmd
                    .args(["create", "--name", VM_NAME, "--tty=false"])
                    .arg(&yaml_path)
                    .output();
                let output = tokio::time::timeout(VM_TIMEOUT_LONG, future)
                    .await
                    .map_err(|_| TerrariumError::VmStartFailed {
                        message: "VM create timed out after 5 minutes".into(),
                    })?
                    .map_err(|e| TerrariumError::VmStartFailed {
                        message: e.to_string(),
                    })?;

                if !output.status.success() {
                    return Err(TerrariumError::VmStartFailed {
                        message: String::from_utf8_lossy(&output.stderr).to_string(),
                    });
                }

                self.start_vm().await
            }
            VmStatus::NotInstalled => Err(TerrariumError::LimaNotInstalled),
            VmStatus::Starting => {
                // Already starting, wait for it
                Ok(())
            }
            VmStatus::Error { message } => Err(TerrariumError::VmStartFailed { message }),
        }
    }

    async fn start_vm(&self) -> Result<(), TerrariumError> {
        let mut cmd = self.limactl()?;
        let future = cmd.args(["start", VM_NAME]).output();

        // VM start can take a while but shouldn't take more than 2 minutes
        let output = tokio::time::timeout(std::time::Duration::from_secs(120), future)
            .await
            .map_err(|_| TerrariumError::VmStartFailed {
                message: "VM start timed out after 120s".into(),
            })?
            .map_err(|e| TerrariumError::VmStartFailed {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(TerrariumError::VmStartFailed {
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    async fn stop_vm(&self) -> Result<(), TerrariumError> {
        let mut cmd = self.limactl()?;
        let future = cmd.args(["stop", VM_NAME]).output();

        let output = tokio::time::timeout(std::time::Duration::from_secs(60), future)
            .await
            .map_err(|_| TerrariumError::LimaCommandFailed {
                message: "VM stop timed out after 60s".into(),
            })?
            .map_err(|e| TerrariumError::LimaCommandFailed {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(TerrariumError::LimaCommandFailed {
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    async fn create_namespace(&self, _project_id: &str) -> Result<(), TerrariumError> {
        // No-op: we use the default namespace with prefixed container/volume names
        // instead of per-project namespaces (avoids expensive image copying).
        Ok(())
    }

    async fn delete_namespace(&self, project_id: &str) -> Result<(), TerrariumError> {
        // Remove the project's container and volume from the default namespace
        let container = Self::container_name(project_id);
        let volume = Self::volume_name(project_id);

        let _ = self.run_nerdctl(None, &["rm", "-f", &container]).await;
        let _ = self.run_nerdctl(None, &["volume", "rm", &volume]).await;

        Ok(())
    }

    async fn namespace_exists(&self, project_id: &str) -> Result<bool, TerrariumError> {
        // Check if the project's container exists in the default namespace
        let container = Self::container_name(project_id);
        let output = self
            .run_nerdctl(None, &["inspect", &container])
            .await?;
        Ok(output.status.success())
    }

    async fn ensure_dev_image(&self) -> Result<(), TerrariumError> {
        // Check if image already exists in default namespace
        let output = self.run_nerdctl(None, &["images", "-q", DEV_IMAGE]).await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                // Image already exists
                return Ok(());
            }
        }

        // Image doesn't exist — build it
        // Build context is the repo root (so Dockerfile can COPY from mcp-server/dist/)
        let context_dir = self.find_repo_root()?;
        let dockerfile_path = self.find_dockerfile()?;

        // The build context must be accessible inside the VM via virtiofs.
        // Lima with virtiofs mounts the user's home directory by default.
        let context_path = context_dir.to_string_lossy();
        let dockerfile_str = dockerfile_path.to_string_lossy();

        let output = self
            .run_nerdctl_timeout(
                None,
                &[
                    "build",
                    "-t",
                    DEV_IMAGE,
                    "-f",
                    &dockerfile_str,
                    &context_path,
                ],
                VM_TIMEOUT_LONG,
            )
            .await?;

        if !output.status.success() {
            return Err(TerrariumError::ImageBuildFailed {
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    async fn load_dev_image_into_namespace(&self, _project_id: &str) -> Result<(), TerrariumError> {
        // No-op: all containers run in the default namespace where the image
        // is already built. No cross-namespace image copy needed.
        Ok(())
    }

    async fn run_dev_container(
        &self,
        project_id: &str,
        workspace_path: &str,
    ) -> Result<(), TerrariumError> {
        let container = Self::container_name(project_id);
        let volume = Self::volume_name(project_id);

        // Check current container status
        let status = self.dev_container_status(project_id).await?;

        match status {
            ContainerStatus::Running => return Ok(()),
            ContainerStatus::Stopped => {
                // Start the existing stopped container
                let output = self.run_nerdctl(None, &["start", &container]).await?;
                if !output.status.success() {
                    return Err(TerrariumError::ContainerError {
                        message: format!(
                            "Failed to start dev container: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
                return Ok(());
            }
            ContainerStatus::NotCreated | ContainerStatus::Unknown { .. } => {
                // Create and run a new container
            }
        }

        // Bind-mount the host workspace directory into the container
        let workspace_mount = format!(
            "{}:/home/terrarium/workspace:rw",
            workspace_path
        );
        let volume_mount = format!("{}:/home/terrarium/.claude", volume);

        let output = self
            .run_nerdctl(
                None,
                &[
                    "run", "-d", "--name", &container,
                    "-v", &volume_mount,
                    "-v", &workspace_mount,
                    DEV_IMAGE,
                ],
            )
            .await?;

        if !output.status.success() {
            return Err(TerrariumError::ContainerError {
                message: format!(
                    "Failed to run dev container: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }

        Ok(())
    }

    async fn remove_dev_container(&self, project_id: &str) -> Result<(), TerrariumError> {
        let container = Self::container_name(project_id);

        let output = self.run_nerdctl(None, &["rm", "-f", &container]).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // Ignore "not found" / "no such container" errors
            if !stderr.contains("not found") && !stderr.contains("no such") {
                return Err(TerrariumError::ContainerError {
                    message: format!("Failed to remove dev container: {}", stderr),
                });
            }
        }

        Ok(())
    }

    async fn dev_container_status(
        &self,
        project_id: &str,
    ) -> Result<ContainerStatus, TerrariumError> {
        let container = Self::container_name(project_id);

        let output = self
            .run_nerdctl(
                None,
                &["inspect", "--format", "{{.State.Status}}", &container],
            )
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("not found") || stderr.contains("no such") {
                return Ok(ContainerStatus::NotCreated);
            }
            return Ok(ContainerStatus::Unknown { message: stderr });
        }

        let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match status_str.as_str() {
            "running" => Ok(ContainerStatus::Running),
            "stopped" | "exited" | "created" => Ok(ContainerStatus::Stopped),
            other => Ok(ContainerStatus::Unknown {
                message: format!("Unknown container status: {}", other),
            }),
        }
    }
}

/// Default timeout for VM commands (30 seconds).
const VM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Longer timeout for image builds (5 minutes).
const VM_TIMEOUT_LONG: std::time::Duration = std::time::Duration::from_secs(300);

impl LimaRuntime {
    /// Run a nerdctl command inside the VM with a timeout.
    /// Returns an error if the command doesn't complete in time (hung VM).
    async fn run_nerdctl(
        &self,
        namespace: Option<&str>,
        args: &[&str],
    ) -> Result<std::process::Output, TerrariumError> {
        self.run_nerdctl_timeout(namespace, args, VM_TIMEOUT).await
    }

    /// Run a nerdctl command with a custom timeout (for long operations like image builds).
    async fn run_nerdctl_timeout(
        &self,
        namespace: Option<&str>,
        args: &[&str],
        timeout: std::time::Duration,
    ) -> Result<std::process::Output, TerrariumError> {
        let mut cmd = self.limactl()?;
        let mut shell_args = vec!["shell", VM_NAME, "--"];

        let mut nerdctl_args = vec!["sudo", "nerdctl"];
        if let Some(ns) = namespace {
            nerdctl_args.push("--namespace");
            nerdctl_args.push(ns);
        }
        nerdctl_args.extend_from_slice(args);
        shell_args.extend_from_slice(&nerdctl_args);

        let future = cmd.args(&shell_args).output();
        match tokio::time::timeout(timeout, future).await {
            Ok(result) => result.map_err(|e| TerrariumError::Internal {
                message: e.to_string(),
            }),
            Err(_) => Err(TerrariumError::Internal {
                message: format!("VM command timed out after {}s", timeout.as_secs()),
            }),
        }
    }

    /// Find the path to Dockerfile.dev-base.
    fn find_dockerfile(&self) -> Result<PathBuf, TerrariumError> {
        let candidates = [
            // Development: next to Cargo.toml (running from src-tauri/)
            std::env::current_dir().ok().map(|d| d.join("Dockerfile.dev-base")),
            // Development: src-tauri directory (running from desktop/)
            std::env::current_dir()
                .ok()
                .map(|d| d.join("src-tauri").join("Dockerfile.dev-base")),
            // Next to the executable (bundled app)
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("Dockerfile.dev-base"))),
            // Tauri resource directory
            std::env::current_exe().ok().and_then(|p| {
                p.parent()
                    .and_then(|d| d.parent())
                    .map(|d| d.join("Resources").join("Dockerfile.dev-base"))
            }),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        Err(TerrariumError::Internal {
            message: "Could not find Dockerfile.dev-base".into(),
        })
    }

    /// Find the repo root directory (build context for Docker).
    /// The repo root contains the `mcp-server/` and `desktop/` directories.
    fn find_repo_root(&self) -> Result<PathBuf, TerrariumError> {
        // Find the Dockerfile first, then derive the repo root from it.
        // Dockerfile lives at desktop/src-tauri/Dockerfile.dev-base,
        // so repo root is two levels up from its parent.
        let dockerfile = self.find_dockerfile()?;
        let dockerfile_dir = dockerfile
            .parent()
            .ok_or_else(|| TerrariumError::Internal {
                message: "Dockerfile has no parent directory".into(),
            })?;

        // In development, Dockerfile is at <repo>/desktop/src-tauri/Dockerfile.dev-base
        // So repo root = dockerfile_dir (src-tauri) -> parent (desktop) -> parent (repo root)
        if let Some(repo_root) = dockerfile_dir.parent().and_then(|d| d.parent()) {
            // Verify it looks like a repo root by checking for mcp-server/
            if repo_root.join("mcp-server").exists() || repo_root.join("desktop").exists() {
                return Ok(repo_root.to_path_buf());
            }
        }

        // For bundled app, fall back to the Dockerfile directory
        // (bundled builds will need a different approach later)
        Ok(dockerfile_dir.to_path_buf())
    }

    fn find_vm_template(&self) -> Result<String, TerrariumError> {
        // Look for the template relative to the executable, or in known locations
        let candidates = [
            // Development: next to Cargo.toml
            std::env::current_dir()
                .ok()
                .map(|d| d.join("lima-terrarium.yaml")),
            // Development: src-tauri directory
            std::env::current_dir()
                .ok()
                .map(|d| d.join("src-tauri").join("lima-terrarium.yaml")),
            // Next to the executable (bundled app)
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("lima-terrarium.yaml"))),
            // Tauri resource directory
            std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.parent()
                        .and_then(|d| d.parent())
                        .map(|d| d.join("Resources").join("lima-terrarium.yaml"))
                }),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.exists() {
                return Ok(candidate.to_string_lossy().to_string());
            }
        }

        Err(TerrariumError::Internal {
            message: "Could not find lima-terrarium.yaml template".into(),
        })
    }

    /// Find the built MCP server JS file.
    pub fn find_mcp_server(&self) -> Result<PathBuf, TerrariumError> {
        let repo_root = self.find_repo_root()?;
        let mcp_path = repo_root.join("mcp-server/dist/terrarium-mcp.js");
        if mcp_path.exists() {
            return Ok(mcp_path);
        }

        // Also check next to executable for bundled app
        let candidates = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("terrarium-mcp.js"))),
            std::env::current_exe().ok().and_then(|p| {
                p.parent()
                    .and_then(|d| d.parent())
                    .map(|d| d.join("Resources").join("terrarium-mcp.js"))
            }),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        Err(TerrariumError::Internal {
            message: "Could not find terrarium-mcp.js. Run `npm run build` in mcp-server/".into(),
        })
    }

    /// Find the hooks directory containing terrarium-proxy.sh.
    pub fn find_hooks_dir(&self) -> Result<PathBuf, TerrariumError> {
        let candidates = [
            // Development: next to Cargo.toml (running from src-tauri/)
            std::env::current_dir().ok().map(|d| d.join("hooks")),
            // Development: src-tauri directory (running from desktop/)
            std::env::current_dir()
                .ok()
                .map(|d| d.join("src-tauri").join("hooks")),
            // Next to the executable (bundled app)
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("hooks"))),
            // Tauri resource directory
            std::env::current_exe().ok().and_then(|p| {
                p.parent()
                    .and_then(|d| d.parent())
                    .map(|d| d.join("Resources").join("hooks"))
            }),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.join("terrarium-proxy.sh").exists() {
                return Ok(candidate.clone());
            }
        }

        Err(TerrariumError::Internal {
            message: "Could not find hooks directory".into(),
        })
    }
}
