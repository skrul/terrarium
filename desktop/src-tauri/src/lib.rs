mod error;
mod host_api;
mod mdns;
mod project;
mod proxy;
mod runtime;

use std::path::PathBuf;
use std::sync::Arc;

use project::{Project, ProjectStatus};
use runtime::lima::LimaRuntime;
use runtime::types::{RuntimeStatus, VmStatus};
use runtime::ContainerRuntime;
use tokio::sync::Mutex;

use tauri::State;
use uuid::Uuid;

pub struct AppState {
    projects: Arc<Mutex<Vec<Project>>>,
    runtime: Arc<LimaRuntime>,
    proxy: Arc<proxy::ProxyManager>,
    tls_manager: Arc<proxy::TlsManager>,
    mdns: Arc<mdns::MdnsRegistrar>,
    host_api_port: u16,
}

impl AppState {
    fn new(
        host_api_port: u16,
        proxy: Arc<proxy::ProxyManager>,
        tls_manager: Arc<proxy::TlsManager>,
        mdns: Arc<mdns::MdnsRegistrar>,
    ) -> Self {
        Self {
            projects: Arc::new(Mutex::new(Vec::new())),
            runtime: Arc::new(LimaRuntime::new()),
            proxy,
            tls_manager,
            mdns,
            host_api_port,
        }
    }
}

/// Get the base directory for all Terrarium project workspaces.
fn terrarium_base_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join("Terrarium")
}

/// Create the workspace directory and config files for a new project.
fn setup_workspace(
    project_id: &str,
    project_name: &str,
    container_name: &str,
    hooks_dir: &PathBuf,
    mcp_server_path: &PathBuf,
) -> Result<PathBuf, String> {
    let workspace = terrarium_base_dir().join(project_name);

    // Create directories
    std::fs::create_dir_all(workspace.join(".terrarium"))
        .map_err(|e| format!("Failed to create .terrarium dir: {}", e))?;
    std::fs::create_dir_all(workspace.join(".claude"))
        .map_err(|e| format!("Failed to create .claude dir: {}", e))?;

    // Write .terrarium/config.json
    let terrarium_config = serde_json::json!({
        "project_id": project_id,
        "container_name": container_name,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(
        workspace.join(".terrarium/config.json"),
        serde_json::to_string_pretty(&terrarium_config).unwrap(),
    )
    .map_err(|e| format!("Failed to write .terrarium/config.json: {}", e))?;

    // Write .claude/settings.json with hooks and permissions
    let hook_script = hooks_dir.join("terrarium-proxy.sh");
    let claude_settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": hook_script.to_string_lossy()
                }]
            }]
        },
        "permissions": {
            "allow": [
                "Bash(*)",
                "Read(*)",
                "Write(*)",
                "Edit(*)",
                "mcp__terrarium(*)"
            ]
        }
    });
    std::fs::write(
        workspace.join(".claude/settings.json"),
        serde_json::to_string_pretty(&claude_settings).unwrap(),
    )
    .map_err(|e| format!("Failed to write .claude/settings.json: {}", e))?;

    // Write .mcp.json at project root (Claude Code reads MCP servers from here)
    let mcp_config = serde_json::json!({
        "mcpServers": {
            "terrarium": {
                "command": "node",
                "args": [mcp_server_path.to_string_lossy()],
                "env": {
                    "TERRARIUM_HOST_API": format!("http://localhost:{}", HOST_API_PORT),
                    "TERRARIUM_PROJECT_ID": project_id,
                    "TERRARIUM_PROJECT_NAME": project_name,
                    "TERRARIUM_WORKSPACE": workspace.to_string_lossy(),
                    "TERRARIUM_CONTAINER_NAME": container_name
                }
            }
        }
    });
    std::fs::write(
        workspace.join(".mcp.json"),
        serde_json::to_string_pretty(&mcp_config).unwrap(),
    )
    .map_err(|e| format!("Failed to write .mcp.json: {}", e))?;

    // Write .claude/settings.local.json to pre-approve MCP servers
    let settings_local = serde_json::json!({
        "enableAllProjectMcpServers": true
    });
    std::fs::write(
        workspace.join(".claude/settings.local.json"),
        serde_json::to_string_pretty(&settings_local).unwrap(),
    )
    .map_err(|e| format!("Failed to write .claude/settings.local.json: {}", e))?;

    // Write .claude/CLAUDE.md
    let claude_md = format!(
        "# {name}\n\n\
         This is a Terrarium project. Your development environment runs inside a sandboxed container.\n\n\
         ## How it works\n\n\
         - **File operations** (read, write, edit, search) work directly on this directory.\n\
         - **Shell commands** (bash) are automatically proxied into your dev container.\n\
         - The container has Node.js, Python, and common dev tools pre-installed.\n\n\
         ## Rules\n\n\
         - **Always use the Bash tool to start servers and run commands.** Never create launch.json or run configurations. \
         All commands must go through Bash so they execute inside the container.\n\
         - Do not create or modify `.claude/launch.json`.\n\n\
         ## Tips\n\n\
         - Run `node -v` or `python3 --version` to verify the container environment.\n\
         - Files you create here are immediately visible inside the container.\n\
         - Use `ls /` to explore the container filesystem.\n",
        name = project_name,
    );
    std::fs::write(workspace.join(".claude/CLAUDE.md"), claude_md)
        .map_err(|e| format!("Failed to write .claude/CLAUDE.md: {}", e))?;

    Ok(workspace)
}

/// Scan ~/Terrarium/ for existing project workspaces and reconstruct Project structs.
fn load_projects() -> Vec<Project> {
    let base = terrarium_base_dir();
    let mut projects = Vec::new();

    let entries = match std::fs::read_dir(&base) {
        Ok(entries) => entries,
        Err(_) => return projects,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let config_path = path.join(".terrarium/config.json");
        let config_data = match std::fs::read_to_string(&config_path) {
            Ok(data) => data,
            Err(_) => continue, // Not a Terrarium project
        };

        let config: serde_json::Value = match serde_json::from_str(&config_data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let project_id = match config.get("project_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let container_name = config
            .get("container_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let _ = container_name; // used implicitly via project_id -> container naming

        let created_at = config
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        projects.push(Project {
            id: project_id,
            name,
            status: ProjectStatus::Stopped,
            created_at,
            workspace_path: path.to_string_lossy().to_string(),
        });
    }

    projects
}

/// Delete the workspace directory for a project.
fn remove_workspace(workspace_path: &str) {
    let path = PathBuf::from(workspace_path);
    if path.exists() {
        let _ = std::fs::remove_dir_all(&path);
    }
}

#[tauri::command]
async fn list_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    Ok(state.projects.lock().await.clone())
}

#[tauri::command]
async fn create_project(name: String, state: State<'_, AppState>) -> Result<Project, String> {
    let id = Uuid::new_v4().to_string();
    let container_name = format!("terrarium-{}-dev", &id);

    // Find hooks directory and MCP server
    let hooks_dir = state
        .runtime
        .find_hooks_dir()
        .map_err(|e| e.to_string())?;
    let mcp_server_path = state
        .runtime
        .find_mcp_server()
        .map_err(|e| e.to_string())?;

    // Create workspace directory with config files
    let workspace_path = setup_workspace(&id, &name, &container_name, &hooks_dir, &mcp_server_path)?;
    let workspace_str = workspace_path.to_string_lossy().to_string();

    // Insert project in Creating state
    let project = Project {
        id: id.clone(),
        name,
        status: ProjectStatus::Creating,
        created_at: chrono::Utc::now().to_rfc3339(),
        workspace_path: workspace_str.clone(),
    };
    state.projects.lock().await.push(project.clone());

    // Ensure VM is ready and create namespace
    if let Err(e) = state.runtime.ensure_vm_ready().await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    if let Err(e) = state.runtime.create_namespace(&id).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Build dev image (fast no-op if already built)
    if let Err(e) = state.runtime.ensure_dev_image().await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Load image into project namespace
    if let Err(e) = state.runtime.load_dev_image_into_namespace(&id).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Run the dev container with workspace bind-mount
    if let Err(e) = state.runtime.run_dev_container(&id, &workspace_str).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Update project status to Running
    let mut projects = state.projects.lock().await;
    if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
        p.status = ProjectStatus::Running;
    }

    let project = projects.iter().find(|p| p.id == id).cloned();
    Ok(project.expect("Project was just inserted"))
}

#[tauri::command]
async fn delete_project(id: String, state: State<'_, AppState>) -> Result<(), String> {
    // Get project info before removing from list
    let project_info = {
        let projects = state.projects.lock().await;
        projects
            .iter()
            .find(|p| p.id == id)
            .map(|p| (p.workspace_path.clone(), p.name.clone()))
    };

    // Remove proxy routes and mDNS records for this project
    if let Some((_, ref project_name)) = project_info {
        state.proxy.remove_project_routes(project_name);
        let _ = state.mdns.deregister(project_name);
    }

    // Remove dev container first (best effort — VM might not be running)
    let _ = state.runtime.remove_dev_container(&id).await;

    // Then delete the namespace
    let _ = state.runtime.delete_namespace(&id).await;

    // Remove workspace directory
    if let Some((path, _)) = project_info {
        remove_workspace(&path);
    }

    state.projects.lock().await.retain(|p| p.id != id);
    Ok(())
}

#[tauri::command]
async fn get_runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    Ok(state.runtime.runtime_status().await)
}

#[tauri::command]
async fn get_vm_status(state: State<'_, AppState>) -> Result<VmStatus, String> {
    Ok(state.runtime.vm_status().await)
}

#[tauri::command]
async fn start_vm(state: State<'_, AppState>) -> Result<(), String> {
    state
        .runtime
        .ensure_vm_ready()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_vm(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.stop_vm().await.map_err(|e| e.to_string())
}

/// Open a project's workspace directory in the default terminal.
#[tauri::command]
async fn open_in_terminal(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let workspace_path = {
        let projects = state.projects.lock().await;
        let project = projects
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Project not found: {}", id))?;
        project.workspace_path.clone()
    };

    // Use macOS `open -a Terminal <path>` to open a new terminal window at the workspace
    std::process::Command::new("open")
        .args(["-a", "Terminal", &workspace_path])
        .status()
        .map_err(|e| format!("Failed to open terminal: {}", e))?;

    Ok(())
}

const HOST_API_PORT: u16 = 7778;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize mDNS registrar
    let mdns = Arc::new(
        mdns::MdnsRegistrar::new().expect("Failed to initialize mDNS registrar"),
    );

    // Initialize TLS manager (local CA)
    let tls_manager = Arc::new(
        proxy::TlsManager::load_or_create().expect("Failed to initialize TLS manager"),
    );

    // Start in-process reverse proxy
    let proxy = Arc::new(
        tauri::async_runtime::block_on(proxy::ProxyManager::start(Arc::clone(&tls_manager)))
            .expect("Failed to start proxy server"),
    );

    // Create shared state for host API
    let host_api_state = Arc::new(host_api::HostApiState {
        proxy: Arc::clone(&proxy),
        tls_manager: Arc::clone(&tls_manager),
        mdns: Arc::clone(&mdns),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_projects,
            create_project,
            delete_project,
            get_runtime_status,
            get_vm_status,
            start_vm,
            stop_vm,
            open_in_terminal,
        ])
        .setup(move |app| {
            use tauri::Manager;
            use tauri::menu::{MenuBuilder, SubmenuBuilder, PredefinedMenuItem};

            // Build native macOS menu
            let app_submenu = SubmenuBuilder::new(app, "Terrarium")
                .item(&PredefinedMenuItem::about(app, Some("About Terrarium"), None)?)
                .separator()
                .item(&PredefinedMenuItem::hide(app, Some("Hide Terrarium"))?)
                .item(&PredefinedMenuItem::hide_others(app, Some("Hide Others"))?)
                .item(&PredefinedMenuItem::show_all(app, Some("Show All"))?)
                .separator()
                .item(&PredefinedMenuItem::quit(app, Some("Quit Terrarium"))?)
                .build()?;

            let edit_submenu = SubmenuBuilder::new(app, "Edit")
                .item(&PredefinedMenuItem::undo(app, None)?)
                .item(&PredefinedMenuItem::redo(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::cut(app, None)?)
                .item(&PredefinedMenuItem::copy(app, None)?)
                .item(&PredefinedMenuItem::paste(app, None)?)
                .item(&PredefinedMenuItem::select_all(app, None)?)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .item(&edit_submenu)
                .build()?;

            app.set_menu(menu)?;

            // Start host API server
            let host_api_port = tauri::async_runtime::block_on(async {
                host_api::start(HOST_API_PORT, host_api_state).await
            })
            .map_err(|e| {
                eprintln!("Failed to start host API: {}", e);
                Box::<dyn std::error::Error>::from(e)
            })?;

            // Ensure ~/Terrarium base directory exists
            let base = terrarium_base_dir();
            if let Err(e) = std::fs::create_dir_all(&base) {
                eprintln!("Failed to create ~/Terrarium: {}", e);
            }

            let app_state = AppState::new(host_api_port, Arc::clone(&proxy), Arc::clone(&tls_manager), Arc::clone(&mdns));

            // Load persisted projects from ~/Terrarium/
            let loaded = load_projects();
            if !loaded.is_empty() {
                eprintln!("Loaded {} project(s) from disk", loaded.len());
            }
            {
                let mut projs = app_state.projects.blocking_lock();
                *projs = loaded;
            }

            app.manage(app_state);

            // Background VM start + project status refresh
            let state = app.state::<AppState>();
            let runtime = state.runtime.clone();
            let projects = Arc::clone(&state.projects);

            tauri::async_runtime::spawn(async move {
                let status = runtime.vm_status().await;
                if status == VmStatus::Stopped {
                    if let Err(e) = runtime.start_vm().await {
                        eprintln!("Failed to start VM: {}", e);
                        return;
                    }
                }

                // Refresh container statuses for loaded projects
                let project_ids: Vec<String> = {
                    projects.lock().await.iter().map(|p| p.id.clone()).collect()
                };

                for id in project_ids {
                    match runtime.dev_container_status(&id).await {
                        Ok(container_status) => {
                            let new_status = match container_status {
                                runtime::types::ContainerStatus::Running => ProjectStatus::Running,
                                runtime::types::ContainerStatus::Stopped => ProjectStatus::Stopped,
                                runtime::types::ContainerStatus::NotCreated => ProjectStatus::Stopped,
                                runtime::types::ContainerStatus::Unknown { .. } => ProjectStatus::Error,
                            };
                            let mut projs = projects.lock().await;
                            if let Some(p) = projs.iter_mut().find(|p| p.id == id) {
                                p.status = new_status;
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to check container status for {}: {}", id, e);
                        }
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            use tauri::Manager;
            if let tauri::RunEvent::Exit = event {
                // In-process proxy stops with the process.
                // mDNS cleanup happens via Drop on the MdnsRegistrar.
                eprintln!("Terrarium shutdown complete");
            }
        });
}
