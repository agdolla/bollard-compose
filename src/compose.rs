use std::path::Path;

use bollard::Docker;

use crate::down::{self, DownOptions};
use crate::error::Result;
use crate::lifecycle;
use crate::logs::{self, LogsOptions};
use crate::ps::{self, ServiceStatus};
use crate::project::Project;
use crate::up::{self, UpOptions};

/// Main entry point for compose operations.
///
/// `ComposeManager` holds a Docker client and a compose project,
/// providing methods for the full compose lifecycle.
pub struct ComposeManager {
    docker: Docker,
    project: Project,
}

impl ComposeManager {
    /// Create a new ComposeManager from an existing Docker client and project.
    pub fn new(docker: Docker, project: Project) -> Self {
        Self { docker, project }
    }

    /// Create a ComposeManager from a compose file path.
    /// Connects to Docker using default settings.
    pub fn from_file(docker: Docker, path: &Path) -> Result<Self> {
        let project = Project::from_file(path)?;
        Ok(Self { docker, project })
    }

    /// Create a ComposeManager from a YAML string.
    pub fn from_str(
        docker: Docker,
        yaml: &str,
        project_name: &str,
        working_dir: &Path,
    ) -> Result<Self> {
        let project = Project::from_str(yaml, project_name, working_dir)?;
        Ok(Self { docker, project })
    }

    /// Get a reference to the project.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Get a reference to the Docker client.
    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    /// Start all services (compose up).
    pub async fn up(&self, options: UpOptions) -> Result<()> {
        up::execute_up(&self.docker, &self.project, &options).await
    }

    /// Stop and remove all services (compose down).
    pub async fn down(&self, options: DownOptions) -> Result<()> {
        down::execute_down(&self.docker, &self.project, &options).await
    }

    /// Stop services.
    pub async fn stop(&self, services: &[String]) -> Result<()> {
        lifecycle::stop_services(&self.docker, &self.project, services).await
    }

    /// Start stopped services.
    pub async fn start(&self, services: &[String]) -> Result<()> {
        lifecycle::start_services(&self.docker, &self.project, services).await
    }

    /// Restart services.
    pub async fn restart(&self, services: &[String]) -> Result<()> {
        lifecycle::restart_services(&self.docker, &self.project, services).await
    }

    /// List service statuses (compose ps).
    pub async fn ps(&self) -> Result<Vec<ServiceStatus>> {
        ps::list_services(&self.docker, &self.project).await
    }

    /// Stream logs from services.
    pub async fn logs(&self, options: LogsOptions) -> Result<()> {
        logs::stream_logs(&self.docker, &self.project, &options).await
    }
}
