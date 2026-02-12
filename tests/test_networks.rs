#![cfg(feature = "integration-tests")]

mod common;

use bollard_compose::UpOptions;
use std::time::Duration;

#[tokio::test]
async fn test_default_network_created() {
    let docker = common::docker();
    let project = common::unique_project("net_default");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // Check that the default network was created
    let expected_net = format!("{project}_default");
    let networks = docker.list_networks(None).await.unwrap();
    let found = networks.iter().any(|n| n.name.as_deref() == Some(&expected_net));
    assert!(found, "default network '{expected_net}' should exist");

    // Verify it's a bridge network
    let net = networks.iter().find(|n| n.name.as_deref() == Some(&expected_net)).unwrap();
    assert_eq!(
        net.driver.as_deref(),
        Some("bridge"),
        "default network should be bridge"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_custom_network() {
    let docker = common::docker();
    let project = common::unique_project("net_custom");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - mynet
networks:
  mynet:
    driver: bridge
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // The network name should be {project}_mynet
    let expected_net = format!("{project}_mynet");
    let networks = docker.list_networks(None).await.unwrap();
    let found = networks.iter().any(|n| n.name.as_deref() == Some(&expected_net));
    assert!(found, "custom network '{expected_net}' should exist, networks: {:?}",
        networks.iter().map(|n| n.name.as_deref()).collect::<Vec<_>>());

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_multi_network_service() {
    let docker = common::docker();
    let project = common::unique_project("net_multi");
    let yaml = r#"
services:
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - frontend
      - backend
networks:
  frontend:
    driver: bridge
  backend:
    driver: bridge
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 1);

    // Inspect container to check connected networks
    let info = common::container_inspect(&docker, &statuses[0].container_name).await;
    let network_settings = info.network_settings.as_ref().unwrap();
    let connected = network_settings.networks.as_ref().unwrap();

    let frontend_net = format!("{project}_frontend");
    let backend_net = format!("{project}_backend");

    assert!(
        connected.contains_key(&frontend_net),
        "container should be connected to '{frontend_net}', got: {:?}",
        connected.keys().collect::<Vec<_>>()
    );
    assert!(
        connected.contains_key(&backend_net),
        "container should be connected to '{backend_net}', got: {:?}",
        connected.keys().collect::<Vec<_>>()
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_network_connectivity() {
    let docker = common::docker();
    let project = common::unique_project("net_conn");
    let yaml = r#"
services:
  server:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - backend
  client:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - backend
networks:
  backend:
    driver: bridge
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    // Wait for containers to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2);

    // Find the client container
    let client = statuses.iter().find(|s| s.service == "client").unwrap();

    // Ping the server by service name (Docker DNS resolves service names on the same network)
    let output = common::exec_in_container(
        &docker,
        &client.container_name,
        vec!["ping", "-c", "1", "-W", "3", "server"],
    )
    .await;

    assert!(
        output.contains("1 packets transmitted, 1 packets received")
            || output.contains("1 packets transmitted, 1 received"),
        "ping to 'server' should succeed, got:\n{output}"
    );

    common::cleanup(&docker, &project).await;
}

#[tokio::test]
async fn test_network_isolation() {
    let docker = common::docker();
    let project = common::unique_project("net_iso");
    let yaml = r#"
services:
  svc_a:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - net1
  svc_b:
    image: alpine:3.19
    command: ["sleep", "3600"]
    networks:
      - net2
networks:
  net1:
    driver: bridge
  net2:
    driver: bridge
"#;
    let manager = common::compose_from_yaml(docker.clone(), yaml, &project);
    common::cleanup(&docker, &project).await;

    manager.up(UpOptions { no_pull: true, ..Default::default() }).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    let statuses = manager.ps().await.unwrap();
    assert_eq!(statuses.len(), 2);

    let svc_a = statuses.iter().find(|s| s.service == "svc_a").unwrap();

    // Ping svc_b from svc_a — should fail since they're on different networks
    let output = common::exec_in_container(
        &docker,
        &svc_a.container_name,
        vec!["ping", "-c", "1", "-W", "2", "svc_b"],
    )
    .await;

    // Ping should fail (either "bad address" for DNS failure, or "100% packet loss")
    let success = output.contains("1 packets transmitted, 1 packets received")
        || output.contains("1 packets transmitted, 1 received");
    assert!(
        !success,
        "ping from svc_a to svc_b should fail (different networks), got:\n{output}"
    );

    common::cleanup(&docker, &project).await;
}
