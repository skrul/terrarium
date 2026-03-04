pub mod lima;
pub mod types;

use std::path::PathBuf;

use async_trait::async_trait;
use types::{ContainerStatus, RuntimeStatus, VmStatus};

use crate::error::TerrariumError;

#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// Check if the runtime prerequisites are installed.
    async fn check_prerequisites(&self) -> Result<(), TerrariumError>;

    /// Get the current VM status.
    async fn vm_status(&self) -> VmStatus;

    /// Get full runtime status including version info.
    async fn runtime_status(&self) -> RuntimeStatus;

    /// Ensure the VM exists and is running. Creates if needed.
    async fn ensure_vm_ready(&self) -> Result<(), TerrariumError>;

    /// Start the VM (must already exist).
    async fn start_vm(&self) -> Result<(), TerrariumError>;

    /// Stop the VM.
    async fn stop_vm(&self) -> Result<(), TerrariumError>;

    /// Create a containerd namespace for a project.
    async fn create_namespace(&self, project_id: &str) -> Result<(), TerrariumError>;

    /// Delete a containerd namespace and all resources within it.
    async fn delete_namespace(&self, project_id: &str) -> Result<(), TerrariumError>;

    /// Check if a namespace exists.
    async fn namespace_exists(&self, project_id: &str) -> Result<bool, TerrariumError>;

    /// Ensure the dev base image is built (in the default namespace).
    async fn ensure_dev_image(&self) -> Result<(), TerrariumError>;

    /// Load the dev base image into a project's namespace.
    async fn load_dev_image_into_namespace(&self, project_id: &str) -> Result<(), TerrariumError>;

    /// Run the dev container in a project's namespace.
    async fn run_dev_container(&self, project_id: &str) -> Result<(), TerrariumError>;

    /// Remove the dev container from a project's namespace.
    async fn remove_dev_container(&self, project_id: &str) -> Result<(), TerrariumError>;

    /// Get the status of the dev container in a project's namespace.
    async fn dev_container_status(&self, project_id: &str) -> Result<ContainerStatus, TerrariumError>;

    /// Get the command (program + args) to open a terminal session in a project's dev container.
    async fn terminal_command(&self, project_id: &str, host_api_url: &str) -> Result<(PathBuf, Vec<String>), TerrariumError>;

    /// Get the host gateway IP as seen from inside the VM.
    async fn host_gateway_ip(&self) -> Result<String, TerrariumError>;
}
