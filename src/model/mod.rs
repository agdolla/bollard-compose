pub mod labels;
pub mod network;
pub mod service;
pub mod types;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use self::network::NetworkConfig;
use self::service::ServiceConfig;

/// Top-level resolved compose configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    /// Services keyed by compose service name.
    pub services: IndexMap<String, ServiceConfig>,
    /// Networks keyed by compose network name.
    pub networks: IndexMap<String, NetworkConfig>,
    /// Named volumes (key = volume name, value currently unused).
    pub volumes: IndexMap<String, VolumeConfig>,
}

/// Top-level volume configuration (placeholder for future expansion).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: Option<String>,
    pub driver: Option<String>,
    pub external: bool,
}
