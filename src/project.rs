use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::ComposeConfig;
use crate::parse;

/// A compose project — a resolved compose configuration with metadata.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project name (used for container/network prefixes).
    pub name: String,
    /// Working directory (used for resolving relative paths).
    pub working_dir: PathBuf,
    /// The resolved compose configuration.
    pub config: ComposeConfig,
    /// Additional environment variables.
    pub env: HashMap<String, String>,
}

impl Project {
    /// Load a project from a compose file path.
    ///
    /// The project name is derived from the parent directory name, and the
    /// working directory is the directory containing the compose file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let working_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = working_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());

        let compose = parse::load_from_file(&path)?;
        let config = parse::convert_compose(&compose, &name, &working_dir)?;

        Ok(Self {
            name,
            working_dir,
            config,
            env: HashMap::new(),
        })
    }

    /// Load a project from a YAML string with an explicit name and working directory.
    pub fn from_str(yaml: &str, name: &str, working_dir: &Path) -> Result<Self> {
        let compose = parse::load_from_str(yaml)?;
        let config = parse::convert_compose(&compose, name, working_dir)?;

        Ok(Self {
            name: name.to_string(),
            working_dir: working_dir.to_path_buf(),
            config,
            env: HashMap::new(),
        })
    }

    /// Get a list of all service names.
    pub fn service_names(&self) -> Vec<&str> {
        self.config
            .services
            .keys()
            .map(|s| s.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_from_str() {
        let yaml = r#"
services:
  app:
    image: alpine
    command: echo hello
"#;
        let project = Project::from_str(yaml, "testproject", Path::new("/tmp")).unwrap();
        assert_eq!(project.name, "testproject");
        assert_eq!(project.service_names(), vec!["app"]);
        let app = &project.config.services["app"];
        assert_eq!(app.image.as_deref(), Some("alpine"));
        assert_eq!(
            app.command,
            Some(vec!["echo".to_string(), "hello".to_string()])
        );
    }
}
