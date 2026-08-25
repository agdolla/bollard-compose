use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A parsed port mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortMapping {
    /// Host IP to bind to (empty string = all interfaces).
    pub host_ip: String,
    /// Host port (may differ from container port).
    pub host_port: u16,
    /// Container port.
    pub container_port: u16,
    /// Protocol: "tcp" or "udp".
    pub protocol: String,
}

/// A volume/bind mount specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Source path on host or volume name.
    pub source: String,
    /// Target path inside the container.
    pub target: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
    /// Type of mount.
    pub mount_type: MountType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountType {
    Bind,
    Volume,
    Tmpfs,
}

/// Restart policy for a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RestartPolicyConfig {
    #[default]
    No,
    Always,
    UnlessStopped,
    OnFailure,
}

impl RestartPolicyConfig {
    pub fn from_str_policy(s: &str) -> Self {
        match s {
            "always" => Self::Always,
            "unless-stopped" => Self::UnlessStopped,
            "on-failure" => Self::OnFailure,
            _ => Self::No,
        }
    }

    pub fn as_docker_str(&self) -> &str {
        match self {
            Self::No => "no",
            Self::Always => "always",
            Self::UnlessStopped => "unless-stopped",
            Self::OnFailure => "on-failure",
        }
    }
}

/// Logging configuration for a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    pub driver: Option<String>,
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthcheckConfig {
    pub test: String,
    pub interval: i64,
    pub start_period: i64,
    pub start_interval: i64,
    pub timeout: i64,
    pub retries: i64,
}
