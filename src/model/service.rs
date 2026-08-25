use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::{HealthcheckConfig, LoggingConfig, PortMapping, RestartPolicyConfig, VolumeMount};

/// Resolved service configuration, ready for container creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// The compose service name (key in the services map).
    pub name: String,
    /// Docker image to use.
    pub image: Option<String>,
    /// Explicit container name (if set, only one instance allowed).
    pub container_name: Option<String>,
    /// Container hostname.
    pub hostname: Option<String>,
    /// Command to run (overrides image CMD).
    pub command: Option<Vec<String>>,
    /// Environment variables.
    pub environment: HashMap<String, Option<String>>,
    /// Published port mappings.
    pub ports: Vec<PortMapping>,
    /// Exposed ports (container-only, no host mapping).
    pub expose: Vec<String>,
    /// Volume/bind mounts.
    pub volumes: Vec<VolumeMount>,
    /// Networks this service is connected to.
    pub networks: Vec<String>,
    /// Restart policy.
    pub restart: RestartPolicyConfig,
    /// Logging configuration.
    pub logging: LoggingConfig,
    /// Labels to apply to the container.
    pub labels: HashMap<String, String>,
    /// Services this service depends on (for dependency ordering).
    pub depends_on: Vec<String>,
    pub healthcheck: Option<HealthcheckConfig>,
}

impl ServiceConfig {
    /// Get the effective image name (service name as fallback).
    pub fn effective_image(&self) -> &str {
        self.image.as_deref().unwrap_or(&self.name)
    }

    /// Get the effective container name for a given project and instance number.
    pub fn effective_container_name(&self, project: &str, number: u32) -> String {
        if let Some(ref name) = self.container_name {
            name.clone()
        } else {
            format!("{project}-{}-{number}", self.name)
        }
    }
}
