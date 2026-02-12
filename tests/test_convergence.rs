#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::UpOptions;
use std::time::Duration;

#[tokio::test]
async fn test_idempotent_up() {
    let docker = common::docker();
    let project = common::unique_project("conv_idempotent");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    // First up
    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let first = manager.ps().await.unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0].container_id.clone();

    // Second up (should be idempotent — same container)
    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let second = manager.ps().await.unwrap();
    assert_eq!(second.len(), 1);
    let second_id = second[0].container_id.clone();

    assert_eq!(
        first_id, second_id,
        "idempotent up should keep the same container (first={first_id}, second={second_id})"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_force_recreate() {
    let docker = common::docker();
    let project = common::unique_project("conv_force");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    // First up
    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let first = manager.ps().await.unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0].container_id.clone();

    // Second up with force_recreate
    manager
        .up(UpOptions {
            no_pull: true,
            force_recreate: true,
            ..Default::default()
        })
        .await
        .unwrap();

    let second = manager.ps().await.unwrap();
    assert_eq!(second.len(), 1);
    let second_id = second[0].container_id.clone();

    assert_ne!(
        first_id, second_id,
        "force_recreate should create a new container (first={first_id}, second={second_id})"
    );

    // Verify the new container is running
    assert_eq!(second[0].state, "running");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_no_recreate_ignores_changes() {
    let docker = common::docker();
    let project = common::unique_project("conv_norec");
    let yaml1 = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
"#;
    let yaml2 = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "7200"]
"#;
    let manager1 = common::compose_from_yaml(docker.clone(), yaml1, &project);
    common::cleanup(&docker, &project).await;

    // First up with original config
    manager1.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let first = manager1.ps().await.unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0].container_id.clone();

    // Second up with changed config but no_recreate
    let manager2 = common::compose_from_yaml(docker.clone(), yaml2, &project);
    manager2
        .up(UpOptions {
            no_pull: true,
            no_recreate: true,
            ..Default::default()
        })
        .await
        .unwrap();

    let second = manager2.ps().await.unwrap();
    assert_eq!(second.len(), 1);
    let second_id = second[0].container_id.clone();

    assert_eq!(
        first_id, second_id,
        "no_recreate should keep the same container despite config change"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_config_change_triggers_recreate() {
    let docker = common::docker();
    let project = common::unique_project("conv_change");
    let yaml1 = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    environment:
      MY_VAR: original
"#;
    let yaml2 = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    environment:
      MY_VAR: changed
"#;
    let manager1 = common::compose_from_yaml(docker.clone(), yaml1, &project);
    common::cleanup(&docker, &project).await;

    // First up
    manager1.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let first = manager1.ps().await.unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0].container_id.clone();

    // Second up with changed config (default behavior: should recreate)
    let manager2 = common::compose_from_yaml(docker.clone(), yaml2, &project);
    manager2.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let second = manager2.ps().await.unwrap();
    assert_eq!(second.len(), 1);
    let second_id = second[0].container_id.clone();

    assert_ne!(
        first_id, second_id,
        "config change should trigger recreate (first={first_id}, second={second_id})"
    );

    // Verify the new container has the new env var
    let output = common::exec_in_container(
        &docker,
        &second[0].container_name,
        vec!["sh", "-c", "echo $MY_VAR"],
    )
    .await;
    assert_eq!(output.trim(), "changed");

    common::cleanup(&docker, &project).await;
}
