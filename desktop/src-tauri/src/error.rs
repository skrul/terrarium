use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
pub enum TerrariumError {
    LimaNotInstalled,
    LimaCommandFailed { message: String },
    VmNotRunning,
    VmStartFailed { message: String },
    NamespaceError { message: String },
    ImageBuildFailed { message: String },
    ContainerError { message: String },
    ProjectNotFound { id: String },
    TerminalError { message: String },
    Internal { message: String },
}

impl fmt::Display for TerrariumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerrariumError::LimaNotInstalled => write!(
                f,
                "Lima is not installed. Install it with: brew install lima"
            ),
            TerrariumError::LimaCommandFailed { message } => {
                write!(f, "Lima command failed: {}", message)
            }
            TerrariumError::VmNotRunning => write!(f, "Terrarium VM is not running"),
            TerrariumError::VmStartFailed { message } => {
                write!(f, "Failed to start VM: {}", message)
            }
            TerrariumError::NamespaceError { message } => {
                write!(f, "Namespace error: {}", message)
            }
            TerrariumError::ImageBuildFailed { message } => {
                write!(f, "Image build failed: {}", message)
            }
            TerrariumError::ContainerError { message } => {
                write!(f, "Container error: {}", message)
            }
            TerrariumError::ProjectNotFound { id } => {
                write!(f, "Project not found: {}", id)
            }
            TerrariumError::TerminalError { message } => {
                write!(f, "Terminal error: {}", message)
            }
            TerrariumError::Internal { message } => write!(f, "Internal error: {}", message),
        }
    }
}

impl std::error::Error for TerrariumError {}

impl From<std::io::Error> for TerrariumError {
    fn from(e: std::io::Error) -> Self {
        TerrariumError::Internal {
            message: e.to_string(),
        }
    }
}
