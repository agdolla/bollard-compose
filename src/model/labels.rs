/// Docker Compose label constants, matching docker/compose v2 conventions.

pub const LABEL_PROJECT: &str = "com.docker.compose.project";
pub const LABEL_SERVICE: &str = "com.docker.compose.service";
pub const LABEL_CONTAINER_NUMBER: &str = "com.docker.compose.container-number";
pub const LABEL_CONFIG_HASH: &str = "com.docker.compose.config-hash";
pub const LABEL_WORKING_DIR: &str = "com.docker.compose.project.working_dir";
pub const LABEL_CONFIG_FILES: &str = "com.docker.compose.project.config_files";
pub const LABEL_VERSION: &str = "com.docker.compose.version";
pub const LABEL_ONEOFF: &str = "com.docker.compose.oneoff";
pub const LABEL_NETWORK: &str = "com.docker.compose.network";

/// Build the standard set of labels for a compose-managed container.
pub fn container_labels(
    project: &str,
    service: &str,
    container_number: u32,
    config_hash: &str,
    working_dir: &str,
) -> std::collections::HashMap<String, String> {
    let mut labels = std::collections::HashMap::new();
    labels.insert(LABEL_PROJECT.to_string(), project.to_string());
    labels.insert(LABEL_SERVICE.to_string(), service.to_string());
    labels.insert(
        LABEL_CONTAINER_NUMBER.to_string(),
        container_number.to_string(),
    );
    labels.insert(LABEL_CONFIG_HASH.to_string(), config_hash.to_string());
    labels.insert(LABEL_WORKING_DIR.to_string(), working_dir.to_string());
    labels.insert(LABEL_ONEOFF.to_string(), "False".to_string());
    labels
}

/// Build the standard set of labels for a compose-managed network.
pub fn network_labels(project: &str, network_name: &str) -> std::collections::HashMap<String, String> {
    let mut labels = std::collections::HashMap::new();
    labels.insert(LABEL_PROJECT.to_string(), project.to_string());
    labels.insert(LABEL_NETWORK.to_string(), network_name.to_string());
    labels
}
