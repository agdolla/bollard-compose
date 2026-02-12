use bollard::Docker;
use tracing::info;

use crate::error::Result;
use crate::graph::DependencyGraph;
use crate::ops::{container, convergence, network};
use crate::project::Project;

/// Options for the `down` operation.
#[derive(Debug, Clone, Default)]
pub struct DownOptions {
    /// Also remove named volumes.
    pub remove_volumes: bool,
    /// Remove images: "local" for locally-built only, "all" for all.
    pub remove_images: Option<String>,
    /// Only stop/remove specific services (empty = all).
    pub services: Vec<String>,
}

/// Execute a compose "down" operation.
pub async fn execute_down(
    docker: &Docker,
    project: &Project,
    options: &DownOptions,
) -> Result<()> {
    let config = &project.config;

    info!(project = %project.name, "starting compose down");

    // Build dependency graph and get reverse order for shutdown
    let graph = DependencyGraph::from_config(config)?;
    let groups = graph.parallel_groups();
    let reversed_groups: Vec<_> = groups.into_iter().rev().collect();

    // Get existing containers for the project
    let existing = container::list_project_containers(docker, &project.name).await?;
    let by_service = convergence::group_containers_by_service(&existing);

    // Stop and remove containers in reverse dependency order
    for group in &reversed_groups {
        let mut handles = Vec::new();

        for service_name in group {
            if !options.services.is_empty() && !options.services.contains(service_name) {
                continue;
            }

            if let Some(containers) = by_service.get(service_name) {
                let docker = docker.clone();
                let containers: Vec<_> = containers.iter().map(|c| (*c).clone()).collect();

                handles.push(tokio::spawn(async move {
                    for c in &containers {
                        if let Some(name) = convergence::container_name(c) {
                            // Stop if running
                            if c.state
                                .as_ref()
                                .map(|s| s.to_string() == "running")
                                .unwrap_or(false)
                            {
                                container::stop_container(&docker, &name).await?;
                            }
                            container::remove_container(&docker, &name).await?;
                        }
                    }
                    Ok::<(), crate::error::ComposeError>(())
                }));
            }
        }

        for handle in handles {
            handle.await.map_err(|e| {
                crate::error::ComposeError::ContainerError {
                    service: "unknown".to_string(),
                    reason: format!("task join error: {e}"),
                }
            })??;
        }
    }

    // Also remove any orphan containers (containers belonging to this project
    // but not in the current config)
    if options.services.is_empty() {
        let remaining = container::list_project_containers(docker, &project.name).await?;
        for c in &remaining {
            if let Some(name) = convergence::container_name(c) {
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
    }

    // Remove networks (only when operating on all services)
    if options.services.is_empty() {
        network::remove_networks(docker, &project.name).await?;
    }

    info!(project = %project.name, "compose down complete");
    Ok(())
}
