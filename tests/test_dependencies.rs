#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::{DownOptions, UpOptions};
use std::time::Duration;

const YAML: &str = r#"
services:
  db:
    image: alpine:3.19
    command: ["sleep", "3600"]
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    depends_on:
      - db
  worker:
    image: alpine:3.19
    command: ["sleep", "3600"]
    depends_on:
      - db
"#;

#[tokio::test]
async fn test_depends_on_ordering() {
    let docker = common::docker();
    let project = common::unique_project("dep_order");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 3, "all 3 services should be running");

    let names: Vec<&str> = statuses.iter().map(|s| s.service.as_str()).collect();
    assert!(names.contains(&"db"), "db should be running");
    assert!(names.contains(&"app"), "app should be running");
    assert!(names.contains(&"worker"), "worker should be running");

    for s in &statuses {
        assert_eq!(s.state, "running", "service {} should be running", s.service);
    }

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_selective_service_up() {
    let docker = common::docker();
    let project = common::unique_project("dep_selective_up");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);
    common::cleanup(&docker, &project).await;

    // Only bring up "app" (not worker or db explicitly)
    manager
        .up(UpOptions {
            no_pull: true,
            services: vec!["app".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let statuses = manager.ps().await.unwrap();

    // With selective up, only "app" should be started
    // (db might also be started if depends_on is resolved, or not — depends on implementation)
    let service_names: Vec<&str> = statuses.iter().map(|s| s.service.as_str()).collect();
    assert!(
        service_names.contains(&"app"),
        "app should be running, got: {service_names:?}"
    );
    assert!(
        !service_names.contains(&"worker"),
        "worker should NOT be running when only 'app' was requested, got: {service_names:?}"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_selective_service_down() {
    let docker = common::docker();
    let project = common::unique_project("dep_selective_down");
    let manager = common::compose_from_yaml(docker.clone(), YAML, &project);
    common::cleanup(&docker, &project).await;

    // Bring everything up
    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 3, "all 3 services should be running initially");

    // Down only the worker
    manager
        .down(DownOptions {
            services: vec!["worker".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let statuses = manager.ps().await.unwrap();
    let service_names: Vec<&str> = statuses.iter().map(|s| s.service.as_str()).collect();

    assert!(
        !service_names.contains(&"worker"),
        "worker should be removed after selective down, got: {service_names:?}"
    );
    assert!(
        service_names.contains(&"db"),
        "db should still exist after only worker was downed, got: {service_names:?}"
    );
    assert!(
        service_names.contains(&"app"),
        "app should still exist after only worker was downed, got: {service_names:?}"
    );

    common::cleanup(&docker, &project).await;
}
