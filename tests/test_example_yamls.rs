#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::{DownOptions, UpOptions};
use std::path::Path;
use std::time::Duration;

const EXAMPLES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");

/// Strip `container_name:`, `restart:`, and `ports:` blocks to avoid
/// collisions and restart loops when tests run concurrently.
/// Since all checks use exec-into-container, host port bindings are unnecessary.
fn sanitize_yaml(yaml: &str) -> String {
    let mut result = Vec::new();
    let mut in_ports_block = false;
    let mut ports_indent = 0;

    for line in yaml.lines() {
        let trimmed = line.trim();

        // Skip container_name and restart policy lines
        if trimmed.starts_with("container_name:") || trimmed.starts_with("restart:") {
            continue;
        }

        // Detect start of a ports: block
        if trimmed == "ports:" {
            ports_indent = line.len() - line.trim_start().len();
            in_ports_block = true;
            continue;
        }

        // Skip lines inside a ports: block (indented deeper than the key)
        if in_ports_block {
            if trimmed.is_empty() {
                result.push(line.to_string());
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if indent > ports_indent {
                continue;
            }
            // We've left the ports block
            in_ports_block = false;
        }

        result.push(line.to_string());
    }

    result.join("\n")
}

fn read_example(filename: &str) -> String {
    let path = Path::new(EXAMPLES_DIR).join(filename);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Parse-only tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_nginx_yaml_parses() {
    let yaml = read_example("compose-nginx.yaml");
    let docker = common::docker();
    let project = common::unique_project("ex_nginx_parse");

    let manager = bollard_compose::ComposeManager::from_str(
        docker,
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let names = manager.project().service_names();
    assert_eq!(names.len(), 1, "expected 1 service, got: {names:?}");
    assert!(names.contains(&"web"), "expected 'web' service");

    let svc = &manager.project().config.services["web"];
    assert_eq!(svc.image.as_deref(), Some("nginx:alpine"));
    assert!(!svc.ports.is_empty(), "expected at least one port mapping");
    assert_eq!(svc.ports[0].container_port, 80);
}

#[tokio::test]
async fn test_wordpress_yaml_parses() {
    let yaml = read_example("compose-wordpress.yaml");
    let docker = common::docker();
    let project = common::unique_project("ex_wp_parse");

    let manager = bollard_compose::ComposeManager::from_str(
        docker,
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let names = manager.project().service_names();
    assert_eq!(names.len(), 2, "expected 2 services, got: {names:?}");
    assert!(names.contains(&"db"), "expected 'db' service");
    assert!(names.contains(&"wordpress"), "expected 'wordpress' service");

    // depends_on
    let wp = &manager.project().config.services["wordpress"];
    assert!(
        wp.depends_on.contains(&"db".to_string()),
        "wordpress should depend on db"
    );

    // env vars
    assert!(
        wp.environment.contains_key("WORDPRESS_DB_HOST"),
        "missing WORDPRESS_DB_HOST env var"
    );

    // named volume
    assert!(
        manager.project().config.volumes.contains_key("db_data"),
        "expected 'db_data' named volume"
    );
}

#[tokio::test]
async fn test_prometheus_yaml_parses() {
    let yaml = read_example("compose-prometheus.yaml");
    let docker = common::docker();
    let project = common::unique_project("ex_prom_parse");

    let manager = bollard_compose::ComposeManager::from_str(
        docker,
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let names = manager.project().service_names();
    assert_eq!(names.len(), 2, "expected 2 services, got: {names:?}");
    assert!(names.contains(&"prometheus"), "expected 'prometheus' service");
    assert!(names.contains(&"grafana"), "expected 'grafana' service");

    // depends_on
    let grafana = &manager.project().config.services["grafana"];
    assert!(
        grafana.depends_on.contains(&"prometheus".to_string()),
        "grafana should depend on prometheus"
    );

    // env vars on grafana
    assert!(
        grafana.environment.contains_key("GF_SECURITY_ADMIN_USER"),
        "missing GF_SECURITY_ADMIN_USER env var"
    );
}

// ---------------------------------------------------------------------------
// Integration tests (start containers)
// ---------------------------------------------------------------------------

/// Find container ID from `manager.ps()` for a given service name.
async fn container_id_for_service(
    manager: &bollard_compose::ComposeManager,
    service: &str,
) -> String {
    let statuses = manager.ps().await.unwrap();
    statuses
        .iter()
        .find(|s| s.service == service)
        .unwrap_or_else(|| panic!("service '{service}' not found in ps output"))
        .container_id
        .clone()
}

#[tokio::test]
async fn test_nginx_example() {
    let yaml = sanitize_yaml(&read_example("compose-nginx.yaml"));
    let docker = common::docker();
    let project = common::unique_project("ex_nginx");

    common::cleanup(&docker, &project).await;

    let manager = bollard_compose::ComposeManager::from_str(
        docker.clone(),
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let up_result = manager.up(UpOptions::default()).await;
    assert!(up_result.is_ok(), "up failed: {up_result:?}");

    // Verify running
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].state, "running");

    let cid = container_id_for_service(&manager, "web").await;

    // Wait for nginx to respond
    let output = common::wait_for_exec_success(
        &docker,
        &cid,
        vec!["wget", "-qO-", "http://localhost:80/"],
        Duration::from_secs(30),
    )
    .await;

    assert!(
        output.contains("Welcome to nginx"),
        "expected nginx welcome page, got: {output}"
    );

    // Tear down
    manager.down(DownOptions::default()).await.unwrap();
    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_wordpress_example() {
    let yaml = sanitize_yaml(&read_example("compose-wordpress.yaml"));
    let docker = common::docker();
    let project = common::unique_project("ex_wp");

    common::cleanup(&docker, &project).await;

    let manager = bollard_compose::ComposeManager::from_str(
        docker.clone(),
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let up_result = manager.up(UpOptions::default()).await;
    assert!(up_result.is_ok(), "up failed: {up_result:?}");

    // Verify both services running
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2, "expected 2 services running");

    let db_cid = container_id_for_service(&manager, "db").await;
    let wp_cid = container_id_for_service(&manager, "wordpress").await;

    // Wait for MySQL to be ready (up to 90s)
    common::wait_for_exec_success(
        &docker,
        &db_cid,
        vec!["mysqladmin", "ping", "-h", "localhost", "-u", "root", "-psomewordpress"],
        Duration::from_secs(90),
    )
    .await;

    // Wait for WordPress to respond (up to 120s)
    common::wait_for_exec_success(
        &docker,
        &wp_cid,
        vec!["curl", "-sf", "http://localhost:80/"],
        Duration::from_secs(120),
    )
    .await;

    // Verify DNS resolution from wordpress → db
    let dns_output = common::exec_in_container(&docker, &wp_cid, vec!["getent", "hosts", "db"]).await;
    assert!(
        !dns_output.is_empty(),
        "expected DNS resolution for 'db' from wordpress container"
    );

    // Tear down with volume removal
    manager
        .down(DownOptions {
            remove_volumes: true,
            ..Default::default()
        })
        .await
        .unwrap();
    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_prometheus_grafana_example() {
    let yaml = sanitize_yaml(&read_example("compose-prometheus.yaml"));
    let docker = common::docker();
    let project = common::unique_project("ex_promgraf");

    common::cleanup(&docker, &project).await;

    let manager = bollard_compose::ComposeManager::from_str(
        docker.clone(),
        &yaml,
        &project,
        Path::new(EXAMPLES_DIR),
    )
    .unwrap();

    let up_result = manager.up(UpOptions::default()).await;
    assert!(up_result.is_ok(), "up failed: {up_result:?}");

    // Verify both services running
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2, "expected 2 services running");

    let prom_cid = container_id_for_service(&manager, "prometheus").await;
    let graf_cid = container_id_for_service(&manager, "grafana").await;

    // Wait for Prometheus ready (up to 30s)
    common::wait_for_exec_success(
        &docker,
        &prom_cid,
        vec!["wget", "-qO-", "http://localhost:9090/-/ready"],
        Duration::from_secs(30),
    )
    .await;

    // Wait for Grafana ready (up to 30s)
    common::wait_for_exec_success(
        &docker,
        &graf_cid,
        vec!["wget", "-qO-", "http://localhost:3000/api/health"],
        Duration::from_secs(30),
    )
    .await;

    // Cross-service connectivity: grafana → prometheus
    let cross_output = common::wait_for_exec_success(
        &docker,
        &graf_cid,
        vec!["wget", "-qO-", "http://prometheus:9090/-/ready"],
        Duration::from_secs(15),
    )
    .await;
    assert!(
        !cross_output.is_empty(),
        "expected grafana to reach prometheus via service name"
    );

    // Verify Grafana env vars are set
    let env_output =
        common::exec_in_container(&docker, &graf_cid, vec!["printenv", "GF_SECURITY_ADMIN_USER"])
            .await;
    assert_eq!(
        env_output.trim(),
        "admin",
        "expected GF_SECURITY_ADMIN_USER=admin"
    );

    // Tear down
    manager.down(DownOptions::default()).await.unwrap();
    common::cleanup(&docker, &project).await;
}
