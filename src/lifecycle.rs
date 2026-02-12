use bollard::Docker;
use tracing::info;

use crate::error::Result;
use crate::ops::{container, convergence};
use crate::project::Project;

/// Stop all (or specific) services.
pub async fn stop_services(
    docker: &Docker,
    project: &Project,
    services: &[String],
) -> Result<()> {
    let existing = container::list_project_containers(docker, &project.name).await?;

    for c in &existing {
        if let Some(name) = convergence::container_name(c) {
            // Filter by service if specified
            if !services.is_empty() {
                if let Some(labels) = &c.labels {
                    let svc = labels
                        .get(crate::model::labels::LABEL_SERVICE)
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    if !services.iter().any(|s| s == svc) {
                        continue;
                    }
                }
            }

            if c.state
                .as_ref()
                .map(|s| s.to_string() == "running")
                .unwrap_or(false)
            {
                container::stop_container(docker, &name).await?;
            }
        }
    }

    info!(project = %project.name, "services stopped");
    Ok(())
}

/// Start all (or specific) stopped services.
pub async fn start_services(
    docker: &Docker,
    project: &Project,
    services: &[String],
) -> Result<()> {
    let existing = container::list_project_containers(docker, &project.name).await?;

    for c in &existing {
        if let Some(name) = convergence::container_name(c) {
            if !services.is_empty() {
                if let Some(labels) = &c.labels {
                    let svc = labels
                        .get(crate::model::labels::LABEL_SERVICE)
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    if !services.iter().any(|s| s == svc) {
                        continue;
                    }
                }
            }

            let is_running = c
                .state
                .as_ref()
                .map(|s| s.to_string() == "running")
                .unwrap_or(false);

            if !is_running {
                container::start_container(docker, &name).await?;
            }
        }
    }

    info!(project = %project.name, "services started");
    Ok(())
}

/// Restart all (or specific) services (stop then start).
pub async fn restart_services(
    docker: &Docker,
    project: &Project,
    services: &[String],
) -> Result<()> {
    stop_services(docker, project, services).await?;
    start_services(docker, project, services).await?;
    info!(project = %project.name, "services restarted");
    Ok(())
}
