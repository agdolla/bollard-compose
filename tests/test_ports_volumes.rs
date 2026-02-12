#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::UpOptions;
use std::time::Duration;

#[tokio::test]
async fn test_port_mapping() {
    let docker = common::docker();
    let project = common::unique_project("pv_port");
    let yaml = r#"
services:
  web:
    image: nginx:alpine
    ports:
      - "8199:80"
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // Wait for nginx to start
    tokio::time::sleep(Duration::from_secs(3)).await;

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].state, "running");

    // Verify the port is mapped by inspecting the container
    let info = common::container_inspect(&docker, &statuses[0].container_name).await;
    let host_config = info.host_config.as_ref().unwrap();
    let port_bindings = host_config.port_bindings.as_ref().unwrap();

    let bindings = port_bindings.get("80/tcp").unwrap().as_ref().unwrap();
    assert!(!bindings.is_empty(), "should have port binding for 80/tcp");
    assert_eq!(
        bindings[0].host_port.as_deref(),
        Some("8199"),
        "host port should be 8199"
    );

    // Try to connect to nginx via HTTP on the mapped port
    // Use a simple TCP connection check via the container itself or inspect port status
    let network_settings = info.network_settings.as_ref().unwrap();
    let ports = network_settings.ports.as_ref().unwrap();
    let port_80 = ports.get("80/tcp").unwrap().as_ref().unwrap();
    assert!(!port_80.is_empty(), "port 80 should be published");
    assert_eq!(port_80[0].host_port.as_deref(), Some("8199"));

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_bind_mount_rw() {
    let docker = common::docker();
    let project = common::unique_project("pv_bind_rw");

    // Create a temp dir for the mount
    let tmp_dir = std::env::temp_dir().join(format!("bctest_{project}"));
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let tmp_path = tmp_dir.to_string_lossy();
    let yaml = format!(
        r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    volumes:
      - {tmp_path}:/mnt/testdata
"#
    );
    let manager = common::compose_from_yaml(docker.clone(), &yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    // Create a file inside the container
    let output = common::exec_in_container(
        &docker,
        &statuses[0].container_name,
        vec!["touch", "/mnt/testdata/hello-from-container"],
    )
    .await;
    let _ = output; // touch produces no output

    // Verify the file exists on the host
    let test_file = tmp_dir.join("hello-from-container");
    assert!(
        test_file.exists(),
        "file should exist on host at {:?}",
        test_file
    );

    // Clean up
    common::cleanup(&docker, &project).await;
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_bind_mount_ro() {
    let docker = common::docker();
    let project = common::unique_project("pv_bind_ro");

    // Create a temp dir for the mount with a test file
    let tmp_dir = std::env::temp_dir().join(format!("bctest_{project}"));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    std::fs::write(tmp_dir.join("existing-file"), "hello").unwrap();

    let tmp_path = tmp_dir.to_string_lossy();
    let yaml = format!(
        r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    volumes:
      - {tmp_path}:/mnt/testdata:ro
"#
    );
    let manager = common::compose_from_yaml(docker.clone(), &yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    // Verify we can read the existing file
    let output = common::exec_in_container(
        &docker,
        &statuses[0].container_name,
        vec!["cat", "/mnt/testdata/existing-file"],
    )
    .await;
    assert_eq!(output.trim(), "hello");

    // Attempt to write should fail
    let output = common::exec_in_container(
        &docker,
        &statuses[0].container_name,
        vec!["sh", "-c", "touch /mnt/testdata/should-fail 2>&1 || echo WRITE_FAILED"],
    )
    .await;

    assert!(
        output.contains("Read-only file system") || output.contains("WRITE_FAILED"),
        "write to ro mount should fail, got:\n{output}"
    );

    // Clean up
    common::cleanup(&docker, &project).await;
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
