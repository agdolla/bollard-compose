#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::{DownOptions, UpOptions};
use std::time::Duration;

const YAML: &str = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
  worker:
    image: alpine:3.19
    command: ["sleep", "3600"]
"#;

#[tokio::test]
async fn test_up_creates_and_starts_containers() {
    let docker = common::docker();
    let project = common::unique_project("lifecycle_up");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);

    // Cleanup before and after
    common::cleanup(&docker, &project).await;

    let result = manager.up(UpOptions { no_pull: true, ..Default::default() }).await;
    assert!(result.is_ok(), "up failed: {result:?}");

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2, "expected 2 services, got {}", statuses.len());

    for s in &statuses {
        assert_eq!(s.state, "running", "service {} is not running: {}", s.service, s.state);
    }

    // Verify both service names are present
    let names: Vec<&str> = statuses.iter().map(|s| s.service.as_str()).collect();
    assert!(names.contains(&"app"), "missing 'app' service");
    assert!(names.contains(&"worker"), "missing 'worker' service");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_down_removes_everything() {
    let docker = common::docker();
    let project = common::unique_project("lifecycle_down");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);

    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // Verify containers are up
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2);

    // Bring down
    manager.down(DownOptions::default()).await.unwrap();

    // Verify no containers remain
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 0, "containers still exist after down");

    // Verify networks are removed
    let networks = docker.list_networks(None).await.unwrap();
    let project_nets: Vec<_> = networks
        .iter()
        .filter(|n| {
            n.labels
                .as_ref()
                .and_then(|l| l.get("com.docker.compose.project"))
                == Some(&project.to_string())
        })
        .collect();
    assert!(project_nets.is_empty(), "networks still exist after down: {project_nets:?}");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_stop_and_start() {
    let docker = common::docker();
    let project = common::unique_project("lifecycle_stopstart");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);

    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // Stop all services
    manager.stop(&[]).await.unwrap();

    // Verify containers exist but are not running
    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2, "containers should still exist after stop");
    for s in &statuses {
        assert_ne!(s.state, "running", "service {} should be stopped", s.service);
    }

    // Start again
    manager.start(&[]).await.unwrap();

    // Give containers a moment to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    let statuses = manager.ps().await.unwrap();
    for s in &statuses {
        assert_eq!(s.state, "running", "service {} should be running after start", s.service);
    }

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_restart() {
    let docker = common::docker();
    let project = common::unique_project("lifecycle_restart");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);

    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let before = manager.ps().await.unwrap();
    assert_eq!(before.len(), 2);

    // Restart
    manager.restart(&[]).await.unwrap();

    // Give containers a moment
    tokio::time::sleep(Duration::from_secs(1)).await;

    let after = manager.ps().await.unwrap();
    assert_eq!(after.len(), 2);
    for s in &after {
        assert_eq!(s.state, "running", "service {} should be running after restart", s.service);
    }

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_ps_returns_correct_info() {
    let docker = common::docker();
    let project = common::unique_project("lifecycle_ps");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);

    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2);

    for s in &statuses {
        // Service name should be either "app" or "worker"
        assert!(
            s.service == "app" || s.service == "worker",
            "unexpected service name: {}",
            s.service
        );

        // Container name should contain the project name
        assert!(
            s.container_name.contains(&project),
            "container name '{}' should contain project '{}'",
            s.container_name,
            project
        );

        // Container ID should be 12 chars
        assert_eq!(
            s.container_id.len(),
            12,
            "container ID should be 12 chars, got '{}'",
            s.container_id
        );

        // Image should reference alpine
        assert!(
            s.image.contains("alpine"),
            "image should contain 'alpine', got '{}'",
            s.image
        );

        // State should be running
        assert_eq!(s.state, "running");
    }

    common::cleanup(&docker, &project).await;
}
