use std::path::Path;

use bollard::Docker;
use bollard_compose::{ComposeManager, DownOptions, UpOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Get compose file path from args, defaulting to docker-compose.yml
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docker-compose.yml".to_string());

    let docker = Docker::connect_with_socket_defaults()?;
    let manager = ComposeManager::from_file(docker, Path::new(&path))?;

    println!(
        "Project: {} ({} services)",
        manager.project().name,
        manager.project().config.services.len()
    );

    // Bring services up
    println!("Starting services...");
    manager.up(UpOptions::default()).await?;

    // Show status
    let statuses = manager.ps().await?;
    println!("\n{:<20} {:<30} {:<10} {}", "SERVICE", "CONTAINER", "STATE", "PORTS");
    for s in &statuses {
        println!(
            "{:<20} {:<30} {:<10} {}",
            s.service, s.container_name, s.state, s.ports
        );
    }

    // Wait for Ctrl-C
    println!("\nPress Ctrl-C to stop...");
    tokio::signal::ctrl_c().await?;

    // Bring services down
    println!("\nStopping services...");
    manager.down(DownOptions::default()).await?;
    println!("Done.");

    Ok(())
}
