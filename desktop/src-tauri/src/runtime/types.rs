use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmStatus {
    NotInstalled,
    NotCreated,
    Stopped,
    Starting,
    Running,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContainerStatus {
    NotCreated,
    Running,
    Stopped,
    Unknown { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub vm_status: VmStatus,
    pub lima_version: Option<String>,
}
