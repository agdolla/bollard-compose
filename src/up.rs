use std::collections::HashMap;

use bollard::Docker;
use tracing::{debug, info, warn};

use crate::error::Result;
use crate::graph::DependencyGraph;
use crate::ops::{container, convergence, image, network};
use crate::project::Project;

/// Options for the `up` operation.
#[derive(Debug, Clone, Default)]
pub struct UpOptions {
    /// Force recreation of containers even if config hasn't changed.
    pub force_recreate: bool,
    /// Never recreate containers (even if config changed).
    pub no_recreate: bool,
    /// Don't pull images before starting.
    pub no_pull: bool,
    /// Only start specific services (empty = all).
    pub services: Vec<String>,
}

/// Execute a compose "up" operation.
pub async fn execute_up(
    docker: &Docker,
    project: &Project,
    options: &UpOptions,
) -> Result<()> {
    let config = &project.config;

    // Build dependency graph
    let graph = DependencyGraph::from_config(config)?;
    let groups = graph.parallel_groups();

    info!(project = %project.name, "starting compose up");

    // Ensure networks exist
    let network_ids = network::ensure_networks(docker, &project.name, &config.networks).await?;
    // If no networks were defined, create a default one
    let network_ids = if config.networks.is_empty() {
        let mut net_ids = network_ids;
        let default_name = format!("{}_default", project.name);
        if !net_ids.contains_key("default") {
            let default_config = crate::model::network::NetworkConfig {
                key: "default".to_string(),
                name: default_name,
                driver: Some("bridge".to_string()),
                external: false,
                labels: HashMap::new(),
            };
            let mut default_nets = indexmap::IndexMap::new();
            default_nets.insert("default".to_string(), default_config);
            let ids = network::ensure_networks(docker, &project.name, &default_nets).await?;
            net_ids.extend(ids);
        }
        net_ids
    } else {
        network_ids
    };

    // Pull missing images
    if !options.no_pull {
        let images: Vec<String> = config
            .services
            .values()
            .filter_map(|s| s.image.clone())
            .collect();
        image::ensure_images(docker, &images).await?;
    }

    // Get existing containers for the project
    let existing = container::list_project_containers(docker, &project.name).await?;
    let by_service = convergence::group_containers_by_service(&existing);

    // Process services in dependency order (group by group)
    for group in &groups {
        let mut handles = Vec::new();

        for service_name in group {
            // Skip services not in the requested list
            if !options.services.is_empty() && !options.services.contains(service_name) {
                continue;
            }

            let service = match config.services.get(service_name) {
                Some(s) => s.clone(),
                None => continue,
            };

            let hash = convergence::config_hash(&service);
            let service_containers: Vec<_> = by_service
                .get(service_name)
                .map(|cs| cs.iter().map(|c| (*c).clone()).collect())
                .unwrap_or_default();

            let state = convergence::check_service_state(&service_containers, &hash);
            let docker = docker.clone();
            let project_name = project.name.clone();
            let working_dir = project.working_dir.to_string_lossy().to_string();
            let net_ids = network_ids.clone();
            let net_configs = config.networks.clone();
            let force_recreate = options.force_recreate;
            let no_recreate = options.no_recreate;

            handles.push(tokio::spawn(async move {
                handle_service_up(
                    &docker,
                    &project_name,
                    &service,
                    &hash,
                    &service_containers,
                    state,
                    &working_dir,
                    &net_ids,
                    &net_configs,
                    force_recreate,
                    no_recreate,
                )
                .await
            }));
        }

        // Wait for all services in this group to complete
        for handle in handles {
            handle.await.map_err(|e| {
                crate::error::ComposeError::ContainerError {
                    service: "unknown".to_string(),
                    reason: format!("task join error: {e}"),
                }
            })??;
        }
    }

    info!(project = %project.name, "compose up complete");
    Ok(())
}

async fn handle_service_up(
    docker: &Docker,
    project_name: &str,
    service: &crate::model::service::ServiceConfig,
    config_hash: &str,
    existing_containers: &[bollard::models::ContainerSummary],
    state: convergence::ServiceState,
    working_dir: &str,
    network_ids: &HashMap<String, String>,
    network_configs: &indexmap::IndexMap<String, crate::model::network::NetworkConfig>,
    force_recreate: bool,
    no_recreate: bool,
) -> Result<()> {
    match state {
        convergence::ServiceState::Running if !force_recreate => {
            debug!(service = %service.name, "service already running with matching config");
            Ok(())
        }
        convergence::ServiceState::Running if force_recreate => {
            info!(service = %service.name, "force recreating running service");
            remove_existing_containers(docker, existing_containers).await?;
            create_and_start(docker, project_name, service, config_hash, working_dir, network_ids, network_configs)
                .await
        }
        convergence::ServiceState::Stopped => {
            info!(service = %service.name, "starting stopped service");
            for c in existing_containers {
                if let Some(name) = convergence::container_name(c) {
                    container::start_container(docker, &name).await?;
                }
            }
            Ok(())
        }
        convergence::ServiceState::Diverged if no_recreate => {
            warn!(service = %service.name, "config changed but no_recreate is set, skipping");
            Ok(())
        }
        convergence::ServiceState::Diverged => {
            info!(service = %service.name, "config changed, recreating service");
            remove_existing_containers(docker, existing_containers).await?;
            create_and_start(docker, project_name, service, config_hash, working_dir, network_ids, network_configs)
                .await
        }
        convergence::ServiceState::Missing => {
            info!(service = %service.name, "creating new service");
            create_and_start(docker, project_name, service, config_hash, working_dir, network_ids, network_configs)
                .await
        }
        _ => Ok(()),
    }
}

async fn create_and_start(
    docker: &Docker,
    project_name: &str,
    service: &crate::model::service::ServiceConfig,
    config_hash: &str,
    working_dir: &str,
    network_ids: &HashMap<String, String>,
    network_configs: &indexmap::IndexMap<String, crate::model::network::NetworkConfig>,
) -> Result<()> {
    let response = container::create_container(
        docker,
        project_name,
        service,
        1,
        config_hash,
        working_dir,
        network_ids,
        network_configs,
    )
    .await?;

    container::start_container(docker, &response.id).await?;

    info!(service = %service.name, "service started");
    Ok(())
}

async fn remove_existing_containers(
    docker: &Docker,
    containers: &[bollard::models::ContainerSummary],
) -> Result<()> {
    for c in containers {
        if let Some(name) = convergence::container_name(c) {
            // Stop first if running
            if c.state
                .as_ref()
                .map(|s| s.to_string() == "running")
                .unwrap_or(false)
            {
                container::stop_container(docker, &name).await?;
            }
            container::remove_container(docker, &name).await?;
        }
    }
    Ok(())
}
