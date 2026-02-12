use std::path::Path;

use docker_compose_types::Compose;

use crate::error::{ComposeError, Result};

/// Load a compose file from a file path.
pub fn load_from_file(path: &Path) -> Result<Compose> {
    if !path.exists() {
        return Err(ComposeError::FileNotFound(path.to_path_buf()));
    }
    let content = std::fs::read_to_string(path)?;
    load_from_str(&content)
}

/// Load a compose file from a YAML string.
pub fn load_from_str(yaml: &str) -> Result<Compose> {
    let compose: Compose =
        serde_yaml::from_str(yaml).map_err(|e| ComposeError::ParseError(e.to_string()))?;
    Ok(compose)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_minimal_yaml() {
        let yaml = r#"
services:
  web:
    image: nginx
"#;
        let compose = load_from_str(yaml).unwrap();
        assert!(!compose.services.0.is_empty());
    }

    #[test]
    fn test_load_invalid_yaml() {
        let yaml = "not: [valid: yaml: {{}}}";
        assert!(load_from_str(yaml).is_err());
    }
}
