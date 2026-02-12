# bollard-compose

Docker Compose functionality for Rust, powered by [bollard](https://github.com/fussybeaver/bollard).

`bollard-compose` is a Rust library that provides Docker Compose-like orchestration using bollard as the underlying Docker client. It parses standard `compose.yaml` files and manages multi-container applications programmatically — creating networks, pulling images, starting containers in dependency order, and tearing everything down.

## Features

- Parse and validate Docker Compose files
- Full service lifecycle: up, down, stop, start, restart
- Dependency ordering via `depends_on`
- Convergence: only recreate containers when configuration changes
- Network management (default and custom networks)
- Port mappings, volume mounts, environment variables
- Container configuration: labels, hostname, command overrides, expose

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
bollard-compose = { path = "path/to/bollard-compose" }
bollard = "0.20"
tokio = { version = "1", features = ["full"] }
```

### From a compose file

Given a standard `compose.yaml`:

```yaml
services:
  web:
    image: nginx:alpine
    ports:
      - "8080:80"
```

Load and run it:

```rust
use std::path::Path;
use bollard::Docker;
use bollard_compose::{ComposeManager, DownOptions, UpOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_socket_defaults()?;
    let manager = ComposeManager::from_file(docker, Path::new("compose.yaml"))?;

    // Start services
    manager.up(UpOptions::default()).await?;

    // Check status
    for s in manager.ps().await? {
        println!("{}: {} ({})", s.service, s.container_name, s.state);
    }

    // Tear down
    manager.down(DownOptions::default()).await?;
    Ok(())
}
```

### From inline YAML

```rust
use std::path::Path;
use bollard::Docker;
use bollard_compose::{ComposeManager, DownOptions, UpOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_socket_defaults()?;

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

    let manager = ComposeManager::from_str(docker, yaml, "myapp", Path::new("."))?;

    manager.up(UpOptions::default()).await?;

    for s in manager.ps().await? {
        println!("{}: {}", s.service, s.state);
    }

    manager.down(DownOptions::default()).await?;
    Ok(())
}
```

## Real-world examples

These examples are inspired by [docker/awesome-compose](https://github.com/docker/awesome-compose). Compose files for each are included in the `examples/` directory.

### WordPress + MySQL

A classic WordPress deployment with MariaDB, demonstrating environment variables, named volumes, exposed ports, and service dependencies.

```yaml
# examples/compose-wordpress.yaml
services:
  db:
    image: mariadb:10.6.4-focal
    command: '--default-authentication-plugin=mysql_native_password'
    volumes:
      - db_data:/var/lib/mysql
    restart: always
    environment:
      - MYSQL_ROOT_PASSWORD=somewordpress
      - MYSQL_DATABASE=wordpress
      - MYSQL_USER=wordpress
      - MYSQL_PASSWORD=wordpress
    expose:
      - "3306"
      - "33060"

  wordpress:
    image: wordpress:latest
    ports:
      - "8080:80"
    restart: always
    environment:
      - WORDPRESS_DB_HOST=db
      - WORDPRESS_DB_USER=wordpress
      - WORDPRESS_DB_PASSWORD=wordpress
      - WORDPRESS_DB_NAME=wordpress
    depends_on:
      - db

volumes:
  db_data:
```

```rust
let manager = ComposeManager::from_file(
    docker,
    Path::new("examples/compose-wordpress.yaml"),
)?;
manager.up(UpOptions::default()).await?;
// WordPress available at http://localhost:8080
```

### Prometheus + Grafana

A monitoring stack with Prometheus metrics collection and Grafana dashboards.

```yaml
# examples/compose-prometheus.yaml
services:
  prometheus:
    image: prom/prometheus
    container_name: prometheus
    ports:
      - "9090:9090"
    restart: unless-stopped

  grafana:
    image: grafana/grafana
    container_name: grafana
    ports:
      - "3000:3000"
    restart: unless-stopped
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=grafana
    depends_on:
      - prometheus
```

## API

### ComposeManager

The main entry point. Create one, then call lifecycle methods:

| Method | Description |
|--------|-------------|
| `from_file(docker, path)` | Load from a compose file |
| `from_str(docker, yaml, name, dir)` | Load from a YAML string |
| `up(options)` | Create and start services |
| `down(options)` | Stop and remove services and networks |
| `stop(services)` | Stop running services |
| `start(services)` | Start stopped services |
| `restart(services)` | Restart services |
| `ps()` | List service statuses |
| `logs(options)` | Stream service logs |

### UpOptions

```rust
UpOptions {
    force_recreate: false, // Recreate even if config unchanged
    no_recreate: false,    // Never recreate, even if config changed
    no_pull: false,        // Don't pull images before starting
    services: vec![],      // Specific services (empty = all)
}
```

### DownOptions

```rust
DownOptions {
    remove_volumes: false,     // Also remove named volumes
    remove_images: None,       // "local" or "all"
    services: vec![],          // Specific services (empty = all)
}
```

## Running the examples

```sh
# Single-service nginx
cargo run --example basic_up -- examples/compose-nginx.yaml

# Multi-service with dependencies
cargo run --example multi_service
```

## License

Apache-2.0
# bollard-compose
