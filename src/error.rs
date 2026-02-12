use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    #[error("failed to parse compose file: {0}")]
    ParseError(String),

    #[error("service not found: {0}")]
    ServiceNotFound(String),

    #[error("circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("docker API error: {0}")]
    DockerError(#[from] bollard::errors::Error),

    #[error("failed to pull image '{image}': {reason}")]
    ImagePullError { image: String, reason: String },

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("invalid port mapping: {0}")]
    InvalidPort(String),

    #[error("invalid volume mount: {0}")]
    InvalidVolume(String),

    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("container error for service '{service}': {reason}")]
    ContainerError { service: String, reason: String },
}

pub type Result<T> = std::result::Result<T, ComposeError>;
