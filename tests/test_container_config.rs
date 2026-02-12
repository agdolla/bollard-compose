#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::UpOptions;
use std::time::Duration;

#[tokio::test]
async fn test_custom_container_name() {
    let docker = common::docker();
    let project = common::unique_project("cfg_cname");
    let custom_name = format!("my-custom-{project}");
    let yaml = format!(
        r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    container_name: {custom_name}
"#
    );
    let manager = common::compose_from_yaml(docker.clone(), &yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].container_name, custom_name);

    // Also verify via Docker inspect
    let info = common::container_inspect(&docker, &custom_name).await;
    // Docker prepends "/" to container names
    let docker_name = info.name.unwrap();
    assert!(
        docker_name.ends_with(&custom_name),
        "Docker name '{docker_name}' should end with '{custom_name}'"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_hostname() {
    let docker = common::docker();
    let project = common::unique_project("cfg_hostname");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    hostname: myspecialhost
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    let output = common::exec_in_container(&docker, &statuses[0].container_name, vec!["hostname"]).await;
    assert_eq!(output.trim(), "myspecialhost");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_command_override() {
    let docker = common::docker();
    let project = common::unique_project("cfg_cmd");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["echo", "hello-world-test"]
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // The container will have exited after echo, check via inspect
    tokio::time::sleep(Duration::from_secs(2)).await;

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    // Read logs to verify output
    use bollard::query_parameters::LogsOptions;
    use futures_util::TryStreamExt;

    let logs = docker
        .logs(
            &statuses[0].container_name,
            Some(LogsOptions {
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        )
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    let output: String = logs.iter().map(|l| l.to_string()).collect();
    assert!(
        output.contains("hello-world-test"),
        "logs should contain 'hello-world-test', got: {output}"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_custom_labels() {
    let docker = common::docker();
    let project = common::unique_project("cfg_labels");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    labels:
      app: test
      env: staging
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    let info = common::container_inspect(&docker, &statuses[0].container_name).await;
    let labels = info.config.as_ref().unwrap().labels.as_ref().unwrap();

    assert_eq!(labels.get("app").map(|s| s.as_str()), Some("test"));
    assert_eq!(labels.get("env").map(|s| s.as_str()), Some("staging"));

    // Also verify compose labels exist alongside custom ones
    assert!(
        labels.contains_key("com.docker.compose.project"),
        "compose project label should be present"
    );
    assert!(
        labels.contains_key("com.docker.compose.service"),
        "compose service label should be present"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_environment_key_value() {
    let docker = common::docker();
    let project = common::unique_project("cfg_env_kv");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    environment:
      FOO: bar
      BAZ: qux
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    let output = common::exec_in_container(&docker, &statuses[0].container_name, vec!["env"]).await;
    assert!(output.contains("FOO=bar"), "env should contain FOO=bar, got:\n{output}");
    assert!(output.contains("BAZ=qux"), "env should contain BAZ=qux, got:\n{output}");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_environment_list_format() {
    let docker = common::docker();
    let project = common::unique_project("cfg_env_list");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    environment:
      - FOO=bar
      - BAZ=qux
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    let output = common::exec_in_container(&docker, &statuses[0].container_name, vec!["env"]).await;
    assert!(output.contains("FOO=bar"), "env should contain FOO=bar, got:\n{output}");
    assert!(output.contains("BAZ=qux"), "env should contain BAZ=qux, got:\n{output}");

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_expose() {
    let docker = common::docker();
    let project = common::unique_project("cfg_expose");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    expose:
      - "8080"
      - "9090"
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    let info = common::container_inspect(&docker, &statuses[0].container_name).await;
    let config = info.config.as_ref().unwrap();

    let exposed = config.exposed_ports.as_ref().unwrap();
    assert!(
        exposed.iter().any(|p| p.contains("8080")),
        "should have 8080 exposed, got: {exposed:?}"
    );
    assert!(
        exposed.iter().any(|p| p.contains("9090")),
        "should have 9090 exposed, got: {exposed:?}"
    );

    common::cleanup(&docker, &project).await;
}
