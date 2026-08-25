use std::collections::HashMap;

use bollard::models::{
    ContainerCreateBody, ContainerCreateResponse, ContainerSummary, EndpointSettings, HostConfig,
    HostConfigLogConfig, NetworkingConfig, PortBinding, PortMap, RestartPolicy,
    RestartPolicyNameEnum,
};
use bollard::plugin::HealthConfig;
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptions, RemoveContainerOptions, StopContainerOptions,
};
use bollard::Docker;
use tracing::{debug, info};

use crate::error::Result;
use crate::model::labels;
use crate::model::network::NetworkConfig;
use crate::model::service::ServiceConfig;
use crate::model::types::{MountType, RestartPolicyConfig};

/// Create a container from a service config.
pub async fn create_container(
    docker: &Docker,
    project: &str,
    service: &ServiceConfig,
    container_number: u32,
    config_hash: &str,
    working_dir: &str,
    network_ids: &HashMap<String, String>,
    network_configs: &indexmap::IndexMap<String, NetworkConfig>,
) -> Result<ContainerCreateResponse> {
    let container_name = service.effective_container_name(project, container_number);

    info!(
        service = %service.name,
        container = %container_name,
        "creating container"
    );

    // Build labels
    let mut all_labels =
        labels::container_labels(project, &service.name, container_number, config_hash, working_dir);
    all_labels.extend(service.labels.clone());

    // Build environment
    let env: Vec<String> = service
        .environment
        .iter()
        .filter_map(|(k, v)| match v {
            Some(val) => Some(format!("{k}={val}")),
            None => std::env::var(k).ok().map(|val| format!("{k}={val}")),
        })
        .collect();

    // Build exposed ports (from ports + expose)
    let mut exposed_ports: Vec<String> = Vec::new();
    for port in &service.ports {
        exposed_ports.push(format!("{}/{}", port.container_port, port.protocol));
    }
    for exp in &service.expose {
        if !exp.contains('/') {
            exposed_ports.push(format!("{exp}/tcp"));
        } else {
            exposed_ports.push(exp.clone());
        }
    }

    // Build port bindings
    let port_bindings = build_port_bindings(service);

    // Build bind mounts
    let binds: Vec<String> = service
        .volumes
        .iter()
        .filter(|v| v.mount_type == MountType::Bind)
        .map(|v| {
            if v.read_only {
                format!("{}:{}:ro", v.source, v.target)
            } else {
                format!("{}:{}", v.source, v.target)
            }
        })
        .collect();

    // Build volume mounts (named volumes)
    let volume_binds: Vec<String> = service
        .volumes
        .iter()
        .filter(|v| v.mount_type == MountType::Volume && !v.source.is_empty())
        .map(|v| format!("{}:{}", v.source, v.target))
        .collect();

    let all_binds: Vec<String> = binds.into_iter().chain(volume_binds).collect();

    // Build restart policy
    let restart_policy = build_restart_policy(&service.restart);

    // Build log config
    let log_config = build_log_config(service);

    // Determine primary network
    let (network_mode, endpoints_config) =
        build_network_config(project, service, network_ids, network_configs);

    let host_config = HostConfig {
        binds: if all_binds.is_empty() {
            None
        } else {
            Some(all_binds)
        },
        port_bindings: Some(port_bindings),
        restart_policy: Some(restart_policy),
        log_config,
        network_mode: Some(network_mode),
        ..Default::default()
    };

    let networking_config = NetworkingConfig {
        endpoints_config: Some(endpoints_config),
    };

    let health_config: Option<HealthConfig> = if !service.healthcheck.is_none() {
        let hc = service.healthcheck.clone().unwrap();
        let test: Vec<String>  = vec!["CMD-SHELL".to_string(), hc.test];
        Some(HealthConfig {
            test: Some(test),
            interval: Some(hc.interval),
            timeout: Some(hc.timeout),
            retries: Some(hc.retries),
            start_period: Some(hc.start_period),
            start_interval: Some(hc.start_interval),
        })
    } else {
        None
    };

    let body = ContainerCreateBody {
        image: service.image.clone(),
        hostname: service.hostname.clone(),
        cmd: service.command.clone(),
        env: if env.is_empty() { None } else { Some(env) },
        exposed_ports: if exposed_ports.is_empty() {
            None
        } else {
            Some(exposed_ports)
        },
        labels: Some(all_labels),
        host_config: Some(host_config),
        networking_config: Some(networking_config),
        healthcheck: health_config,
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(container_name),
        ..Default::default()
    };

    let response = docker.create_container(Some(options), body).await?;
    debug!(id = %response.id, "container created");

    Ok(response)
}

/// Start a container by name or ID.
pub async fn start_container(docker: &Docker, container_name: &str) -> Result<()> {
    info!(container = %container_name, "starting container");
    docker
        .start_container(container_name, None)
        .await?;
    Ok(())
}

/// Stop a container by name or ID.
pub async fn stop_container(docker: &Docker, container_name: &str) -> Result<()> {
    info!(container = %container_name, "stopping container");
    docker
        .stop_container(
            container_name,
            Some(StopContainerOptions {
                t: Some(10),
                ..Default::default()
            }),
        )
        .await?;
    Ok(())
}

/// Remove a container by name or ID.
pub async fn remove_container(docker: &Docker, container_name: &str) -> Result<()> {
    info!(container = %container_name, "removing container");
    docker
        .remove_container(
            container_name,
            Some(RemoveContainerOptions {
                force: true,
                v: true,
                ..Default::default()
            }),
        )
        .await?;
    Ok(())
}

/// List all containers belonging to a project.
pub async fn list_project_containers(
    docker: &Docker,
    project: &str,
) -> Result<Vec<ContainerSummary>> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![format!("{}={}", labels::LABEL_PROJECT, project)],
    );

    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    Ok(containers)
}

/// List containers for a specific service within a project.
pub async fn list_service_containers(
    docker: &Docker,
    project: &str,
    service: &str,
) -> Result<Vec<ContainerSummary>> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![
            format!("{}={}", labels::LABEL_PROJECT, project),
            format!("{}={}", labels::LABEL_SERVICE, service),
        ],
    );

    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;
    Ok(containers)
}

fn build_port_bindings(service: &ServiceConfig) -> PortMap {
    let mut port_bindings: PortMap = HashMap::new();

    for port in &service.ports {
        let key = format!("{}/{}", port.container_port, port.protocol);
        let binding = PortBinding {
            host_ip: if port.host_ip.is_empty() {
                None
            } else {
                Some(port.host_ip.clone())
            },
            host_port: Some(port.host_port.to_string()),
        };

        port_bindings
            .entry(key)
            .or_insert_with(|| Some(Vec::new()))
            .as_mut()
            .unwrap()
            .push(binding);
    }

    port_bindings
}

fn build_restart_policy(policy: &RestartPolicyConfig) -> RestartPolicy {
    let name = match policy {
        RestartPolicyConfig::No => RestartPolicyNameEnum::NO,
        RestartPolicyConfig::Always => RestartPolicyNameEnum::ALWAYS,
        RestartPolicyConfig::UnlessStopped => RestartPolicyNameEnum::UNLESS_STOPPED,
        RestartPolicyConfig::OnFailure => RestartPolicyNameEnum::ON_FAILURE,
    };
    RestartPolicy {
        name: Some(name),
        maximum_retry_count: None,
    }
}

fn build_log_config(service: &ServiceConfig) -> Option<HostConfigLogConfig> {
    let logging = &service.logging;
    if logging.driver.is_none() && logging.options.is_empty() {
        return None;
    }

    Some(HostConfigLogConfig {
        typ: logging.driver.clone(),
        config: if logging.options.is_empty() {
            None
        } else {
            Some(logging.options.clone())
        },
    })
}

/// Resolve the Docker network name for a compose network key.
fn resolve_network_name(
    key: &str,
    project: &str,
    network_configs: &indexmap::IndexMap<String, NetworkConfig>,
) -> String {
    network_configs
        .get(key)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| format!("{project}_{key}"))
}

fn build_network_config(
    project: &str,
    service: &ServiceConfig,
    _network_ids: &HashMap<String, String>,
    network_configs: &indexmap::IndexMap<String, NetworkConfig>,
) -> (String, HashMap<String, EndpointSettings>) {
    let mut endpoints: HashMap<String, EndpointSettings> = HashMap::new();

    if service.networks.is_empty() {
        // Default network
        let default_net = format!("{project}_default");
        endpoints.insert(
            default_net.clone(),
            EndpointSettings {
                aliases: Some(vec![service.name.clone()]),
                ..Default::default()
            },
        );
        return (default_net, endpoints);
    }

    // Use the first network as the primary (network_mode)
    let primary = &service.networks[0];
    let primary_docker_name = resolve_network_name(primary, project, network_configs);

    for net_key in &service.networks {
        let docker_name = resolve_network_name(net_key, project, network_configs);
        endpoints.insert(
            docker_name,
            EndpointSettings {
                aliases: Some(vec![service.name.clone()]),
                ..Default::default()
            },
        );
    }

    (primary_docker_name, endpoints)
}
