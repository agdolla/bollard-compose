use std::path::Path;

use bollard::Docker;
use bollard_compose::{ComposeManager, DownOptions, UpOptions};

/// Demonstrates bringing up a multi-service application from inline YAML,
/// inspecting service status, and tearing it down.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let docker = Docker::connect_with_socket_defaults()?;

    // Define a compose application inline
    let yaml = r#"
services:
  web:
    image: nginx:alpine
    ports:
      - "8090:80"
  cache:
    image: redis:alpine
  app:
    image: alpine:3.19
    command: ["sleep", "3600"]
    depends_on:
      - cache
    environment:
      - REDIS_HOST=cache
"#;

    let manager = ComposeManager::from_str(
        docker,
        yaml,
        "myapp",
        Path::new("."),
    )?;

    println!("Services: {:?}", manager.project().service_names());

    // Start all services
    println!("\n=> Starting services...");
    manager.up(UpOptions::default()).await?;

    // Show status
    let statuses = manager.ps().await?;
    println!("\n{:<15} {:<30} {:<10}", "SERVICE", "CONTAINER", "STATE");
    for s in &statuses {
        println!("{:<15} {:<30} {:<10}", s.service, s.container_name, s.state);
    }

    // Bring everything down
    println!("\n=> Tearing down...");
    manager.down(DownOptions::default()).await?;
    println!("Done.");

    Ok(())
}
