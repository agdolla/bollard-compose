use std::collections::HashMap;

use bollard::models::{Network, NetworkCreateRequest, NetworkConnectRequest, EndpointSettings};
use bollard::Docker;
use tracing::{debug, info};

use crate::error::Result;
use crate::model::labels;
use crate::model::network::NetworkConfig;

/// Ensure all project networks exist. Returns a map of network key → Docker network ID.
pub async fn ensure_networks(
    docker: &Docker,
    project: &str,
    networks: &indexmap::IndexMap<String, NetworkConfig>,
) -> Result<HashMap<String, String>> {
    let existing = list_project_networks(docker, project).await?;
    let existing_names: HashMap<String, String> = existing
        .into_iter()
        .filter_map(|n| {
            let name = n.name?;
            let id = n.id?;
            Some((name, id))
        })
        .collect();

    let mut result = HashMap::new();

    for (key, config) in networks {
        if config.external {
            debug!(network = %config.name, "using external network");
            // For external networks, just verify they exist
            let id = match existing_names.get(&config.name) {
                Some(id) => id.clone(),
                None => {
                    // Try to inspect it — it might exist but not be labeled
                    let info = docker.inspect_network(&config.name, None).await?;
                    info.id.unwrap_or_default()
                }
            };
            result.insert(key.clone(), id);
            continue;
        }

        if let Some(id) = existing_names.get(&config.name) {
            debug!(network = %config.name, "network already exists");
            result.insert(key.clone(), id.clone());
        } else {
            info!(network = %config.name, driver = ?config.driver, "creating network");
            let mut net_labels = labels::network_labels(project, key);
            net_labels.extend(config.labels.clone());

            let response = docker
                .create_network(NetworkCreateRequest {
                    name: config.name.clone(),
                    driver: config.driver.clone(),
                    labels: Some(net_labels),
                    ..Default::default()
                })
                .await?;

            result.insert(key.clone(), response.id);
        }
    }

    Ok(result)
}

/// List all networks belonging to a project (by label).
pub async fn list_project_networks(docker: &Docker, project: &str) -> Result<Vec<Network>> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![format!("{}={}", labels::LABEL_PROJECT, project)],
    );

    let options = bollard::query_parameters::ListNetworksOptions {
        filters: Some(filters),
    };

    let networks = docker.list_networks(Some(options)).await?;
    Ok(networks)
}

/// Remove all project networks.
pub async fn remove_networks(docker: &Docker, project: &str) -> Result<()> {
    let networks = list_project_networks(docker, project).await?;
    for network in networks {
        if let Some(name) = &network.name {
            info!(network = %name, "removing network");
            docker.remove_network(name).await?;
        }
    }
    Ok(())
}

/// Connect a container to a network with optional aliases.
pub async fn connect_container_to_network(
    docker: &Docker,
    network_name: &str,
    container_id: &str,
    aliases: Vec<String>,
) -> Result<()> {
    let config = NetworkConnectRequest {
        container: container_id.to_string(),
        endpoint_config: Some(EndpointSettings {
            aliases: if aliases.is_empty() {
                None
            } else {
                Some(aliases)
            },
            ..Default::default()
        }),
    };

    docker.connect_network(network_name, config).await?;
    Ok(())
}
