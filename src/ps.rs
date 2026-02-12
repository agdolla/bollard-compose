use bollard::Docker;

use crate::error::Result;
use crate::model::labels;
use crate::ops::{container, convergence};
use crate::project::Project;

/// Status information for a service's container.
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub service: String,
    pub container_name: String,
    pub container_id: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: String,
}

/// List the status of all services in a project.
pub async fn list_services(
    docker: &Docker,
    project: &Project,
) -> Result<Vec<ServiceStatus>> {
    let containers = container::list_project_containers(docker, &project.name).await?;

    let mut statuses = Vec::new();

    for c in &containers {
        let name = convergence::container_name(c).unwrap_or_else(|| "unknown".to_string());
        let service = c
            .labels
            .as_ref()
            .and_then(|l| l.get(labels::LABEL_SERVICE))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let state = c
            .state
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let status_str = c.status.clone().unwrap_or_default();
        let image = c.image.clone().unwrap_or_default();
        let container_id = c
            .id
            .as_ref()
            .map(|id| id.chars().take(12).collect())
            .unwrap_or_default();

        let ports = format_ports(c);

        statuses.push(ServiceStatus {
            service,
            container_name: name,
            container_id,
            image,
            state,
            status: status_str,
            ports,
        });
    }

    // Sort by service name
    statuses.sort_by(|a, b| a.service.cmp(&b.service));

    Ok(statuses)
}

fn format_ports(container: &bollard::models::ContainerSummary) -> String {
    container
        .ports
        .as_ref()
        .map(|ports| {
            ports
                .iter()
                .map(|p| {
                    let private = p.private_port;
                    let typ = p.typ.as_ref().map(|t| t.to_string()).unwrap_or_default();
                    if let Some(public) = p.public_port {
                        let ip = p.ip.as_deref().unwrap_or("0.0.0.0");
                        format!("{ip}:{public}->{private}/{typ}")
                    } else {
                        format!("{private}/{typ}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}
