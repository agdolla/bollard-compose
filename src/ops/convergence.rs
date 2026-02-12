use std::collections::HashMap;

use bollard::models::ContainerSummary;
use sha2::{Digest, Sha256};

use crate::model::labels;
use crate::model::service::ServiceConfig;

/// The state of a service relative to its desired configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    /// No containers exist for this service.
    Missing,
    /// Containers exist but their config hash doesn't match (needs recreate).
    Diverged,
    /// Containers exist with matching config but are stopped.
    Stopped,
    /// Containers exist, config matches, and they are running.
    Running,
}

/// Check the state of a service by comparing existing containers against config.
pub fn check_service_state(
    containers: &[ContainerSummary],
    config_hash: &str,
) -> ServiceState {
    if containers.is_empty() {
        return ServiceState::Missing;
    }

    // Check if any container has a different config hash
    for container in containers {
        if let Some(ref labels_map) = container.labels {
            if let Some(existing_hash) = labels_map.get(labels::LABEL_CONFIG_HASH) {
                if existing_hash != config_hash {
                    return ServiceState::Diverged;
                }
            } else {
                // No config hash label — treat as diverged
                return ServiceState::Diverged;
            }
        }
    }

    // Check if all containers are running
    let all_running = containers.iter().all(|c| {
        c.state
            .as_ref()
            .map(|s| s.to_string() == "running")
            .unwrap_or(false)
    });

    if all_running {
        ServiceState::Running
    } else {
        ServiceState::Stopped
    }
}

/// Compute a stable SHA-256 hash of a service's configuration for change detection.
pub fn config_hash(service: &ServiceConfig) -> String {
    let serialized = serde_json::to_string(service).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract a map of service name → list of containers from a flat container list.
pub fn group_containers_by_service(
    containers: &[ContainerSummary],
) -> HashMap<String, Vec<&ContainerSummary>> {
    let mut groups: HashMap<String, Vec<&ContainerSummary>> = HashMap::new();

    for container in containers {
        if let Some(ref labels_map) = container.labels {
            if let Some(service_name) = labels_map.get(labels::LABEL_SERVICE) {
                groups
                    .entry(service_name.clone())
                    .or_default()
                    .push(container);
            }
        }
    }

    groups
}

/// Get the container name (first name, stripped of leading '/').
pub fn container_name(container: &ContainerSummary) -> Option<String> {
    container.names.as_ref().and_then(|names| {
        names
            .first()
            .map(|n| n.strip_prefix('/').unwrap_or(n).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::service::ServiceConfig;
    use crate::model::types::{LoggingConfig, RestartPolicyConfig};

    fn make_service(name: &str, image: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            image: Some(image.to_string()),
            container_name: None,
            hostname: None,
            command: None,
            environment: std::collections::HashMap::new(),
            ports: vec![],
            expose: vec![],
            volumes: vec![],
            networks: vec![],
            restart: RestartPolicyConfig::No,
            logging: LoggingConfig::default(),
            labels: std::collections::HashMap::new(),
            depends_on: vec![],
        }
    }

    #[test]
    fn test_config_hash_stable() {
        let svc = make_service("web", "nginx:latest");
        let hash1 = config_hash(&svc);
        let hash2 = config_hash(&svc);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_config_hash_changes_on_modification() {
        let svc1 = make_service("web", "nginx:latest");
        let mut svc2 = svc1.clone();
        svc2.image = Some("nginx:1.25".to_string());
        assert_ne!(config_hash(&svc1), config_hash(&svc2));
    }

    #[test]
    fn test_check_service_state_missing() {
        assert_eq!(check_service_state(&[], "abc"), ServiceState::Missing);
    }

    #[test]
    fn test_check_service_state_running() {
        let mut labels_map = std::collections::HashMap::new();
        labels_map.insert(labels::LABEL_CONFIG_HASH.to_string(), "abc".to_string());

        let container = ContainerSummary {
            labels: Some(labels_map),
            state: Some(bollard::models::ContainerSummaryStateEnum::RUNNING),
            ..Default::default()
        };

        assert_eq!(
            check_service_state(&[container], "abc"),
            ServiceState::Running
        );
    }

    #[test]
    fn test_check_service_state_diverged() {
        let mut labels_map = std::collections::HashMap::new();
        labels_map.insert(
            labels::LABEL_CONFIG_HASH.to_string(),
            "old_hash".to_string(),
        );

        let container = ContainerSummary {
            labels: Some(labels_map),
            state: Some(bollard::models::ContainerSummaryStateEnum::RUNNING),
            ..Default::default()
        };

        assert_eq!(
            check_service_state(&[container], "new_hash"),
            ServiceState::Diverged
        );
    }

    #[test]
    fn test_check_service_state_stopped() {
        let mut labels_map = std::collections::HashMap::new();
        labels_map.insert(labels::LABEL_CONFIG_HASH.to_string(), "abc".to_string());

        let container = ContainerSummary {
            labels: Some(labels_map),
            state: Some(bollard::models::ContainerSummaryStateEnum::EXITED),
            ..Default::default()
        };

        assert_eq!(
            check_service_state(&[container], "abc"),
            ServiceState::Stopped
        );
    }
}
