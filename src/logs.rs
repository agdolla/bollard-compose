use bollard::Docker;
use futures_util::StreamExt;

use crate::error::Result;
use crate::ops::container;
use crate::ops::convergence;

/// Options for streaming logs.
#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
    /// Follow log output (tail -f).
    pub follow: bool,
    /// Number of lines to show from the end of logs. None = all.
    pub tail: Option<String>,
    /// Show timestamps.
    pub timestamps: bool,
    /// Only show specific services (empty = all).
    pub services: Vec<String>,
}

/// Stream logs from all (or specific) service containers, printing to stdout.
pub async fn stream_logs(
    docker: &Docker,
    project: &crate::project::Project,
    options: &LogsOptions,
) -> Result<()> {
    let existing = container::list_project_containers(docker, &project.name).await?;

    let mut handles = Vec::new();

    for c in &existing {
        let name = match convergence::container_name(c) {
            Some(n) => n,
            None => continue,
        };

        // Filter by service
        let service = c
            .labels
            .as_ref()
            .and_then(|l| l.get(crate::model::labels::LABEL_SERVICE))
            .cloned()
            .unwrap_or_default();

        if !options.services.is_empty() && !options.services.iter().any(|s| s == &service) {
            continue;
        }

        let docker = docker.clone();
        let follow = options.follow;
        let tail = options.tail.clone().unwrap_or_else(|| "all".to_string());
        let timestamps = options.timestamps;
        let service_name = service.clone();

        handles.push(tokio::spawn(async move {
            let log_options = bollard::query_parameters::LogsOptions {
                follow,
                stdout: true,
                stderr: true,
                tail,
                timestamps,
                ..Default::default()
            };

            let mut stream = docker.logs(&name, Some(log_options));

            while let Some(result) = stream.next().await {
                match result {
                    Ok(output) => {
                        print!("{service_name} | {output}");
                    }
                    Err(e) => {
                        eprintln!("{service_name} | Error reading logs: {e}");
                        break;
                    }
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
