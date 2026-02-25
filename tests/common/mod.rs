#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerInspectResponse;
use bollard::Docker;
use futures_util::StreamExt;
use futures_util::TryStreamExt;

use bollard_compose::ComposeManager;

/// Connect to the local Docker daemon via default socket.
pub fn docker() -> Docker {
    Docker::connect_with_socket_defaults().unwrap()
}

/// Generate a unique project name to avoid test collisions.
pub fn unique_project(prefix: &str) -> String {
    let suffix: u32 = rand_suffix();
    format!("bctest_{prefix}_{suffix}")
}

fn rand_suffix() -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    (hasher.finish() % 1_000_000) as u32
}

/// Best-effort cleanup: remove all containers and networks for the project.
pub async fn cleanup(docker: &Docker, project_name: &str) {
    use bollard::query_parameters::{ListContainersOptions, RemoveContainerOptions};
    use std::collections::HashMap;

    // List all containers with the project label
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![format!("com.docker.compose.project={project_name}")],
    );
    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    if let Ok(containers) = docker.list_containers(Some(options)).await {
        for c in &containers {
            if let Some(id) = &c.id {
                let _ = docker
                    .remove_container(
                        id,
                        Some(RemoveContainerOptions {
                            force: true,
                            v: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        }
    }

    // Remove project networks
    if let Ok(networks) = docker.list_networks(None).await {
        for net in &networks {
            if let Some(labels) = &net.labels {
                if labels.get("com.docker.compose.project") == Some(&project_name.to_string()) {
                    if let Some(name) = &net.name {
                        let _ = docker.remove_network(name).await;
                    }
                }
            }
        }
    }
}

/// Create a ComposeManager from inline YAML and a project name.
pub fn compose_from_yaml(
    docker: Docker,
    yaml: &str,
    project_name: &str,
) -> ComposeManager {
    ComposeManager::from_str(docker, yaml, project_name, Path::new("/tmp")).unwrap()
}

/// Inspect a container by name, returning full details.
pub async fn container_inspect(
    docker: &Docker,
    container_name: &str,
) -> ContainerInspectResponse {
    docker.inspect_container(container_name, None).await.unwrap()
}

/// Execute a command inside a running container and return stdout as a String.
pub async fn exec_in_container(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
) -> String {
    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let result = docker.start_exec(&exec.id, None).await.unwrap();

    match result {
        StartExecResults::Attached { output, .. } => {
            let mut stdout = String::new();
            let items: Vec<LogOutput> = output.try_collect().await.unwrap();
            for item in items {
                match item {
                    LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
            stdout
        }
        StartExecResults::Detached => String::new(),
    }
}

/// Poll until a container is in "running" state, up to `timeout`.
pub async fn wait_for_running(
    docker: &Docker,
    container_name: &str,
    timeout: Duration,
) {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for container '{container_name}' to be running"
            );
        }
        if let Ok(info) = docker.inspect_container(container_name, None).await {
            if let Some(state) = &info.state {
                if state.running == Some(true) {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Poll by executing `cmd` inside `container_id` until it exits with code 0.
///
/// Returns the stdout/stderr output of the successful execution.
/// Panics if the timeout is exceeded.
pub async fn wait_for_exec_success(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
    timeout: Duration,
) -> String {
    let start = std::time::Instant::now();
    let label = cmd.join(" ");
    loop {
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for exec success in '{container_id}': {label}"
            );
        }

        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(cmd.clone()),
                    ..Default::default()
                },
            )
            .await;

        let exec = match exec {
            Ok(e) => e,
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let result = docker.start_exec(&exec.id, None).await;
        let output = match result {
            Ok(StartExecResults::Attached { output, .. }) => {
                let mut buf = String::new();
                let items: Vec<_> = output.collect().await;
                for item in items {
                    match item {
                        Ok(LogOutput::StdOut { message }) => {
                            buf.push_str(&String::from_utf8_lossy(&message));
                        }
                        Ok(LogOutput::StdErr { message }) => {
                            buf.push_str(&String::from_utf8_lossy(&message));
                        }
                        _ => {}
                    }
                }
                buf
            }
            Ok(StartExecResults::Detached) => String::new(),
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        // Check exit code
        if let Ok(inspect) = docker.inspect_exec(&exec.id).await {
            if inspect.exit_code == Some(0) {
                return output;
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
