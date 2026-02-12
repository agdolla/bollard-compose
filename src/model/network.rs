use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Resolved network configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// The compose key name (e.g. "backnet").
    pub key: String,
    /// The actual Docker network name (may include project prefix).
    pub name: String,
    /// Network driver (e.g. "bridge", "overlay").
    pub driver: Option<String>,
    /// Whether this is an external network (not managed by compose).
    pub external: bool,
    /// Labels to apply to the network.
    pub labels: HashMap<String, String>,
}

impl NetworkConfig {
    /// Resolve the Docker network name, applying project prefix if no explicit name is set.
    pub fn resolve_name(key: &str, name: Option<&str>, project: &str) -> String {
        match name {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => format!("{project}_{key}"),
        }
    }
}
