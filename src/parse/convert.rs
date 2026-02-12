use std::collections::HashMap;
use std::path::Path;

use docker_compose_types::{
    AdvancedNetworks, AdvancedVolumes, Command, Compose, DependsOnOptions, Environment, Labels,
    LoggingParameters, MapOrEmpty, NetworkSettings, Networks, Port, Ports, PublishedPort,
    SingleValue, Volumes,
};
use indexmap::IndexMap;

use crate::error::{ComposeError, Result};
use crate::model::network::NetworkConfig;
use crate::model::service::ServiceConfig;
use crate::model::types::{
    LoggingConfig, MountType, PortMapping, RestartPolicyConfig, VolumeMount,
};
use crate::model::{ComposeConfig, VolumeConfig};

/// Convert a parsed `docker_compose_types::Compose` into our internal `ComposeConfig`.
pub fn convert_compose(
    compose: &Compose,
    project_name: &str,
    working_dir: &Path,
) -> Result<ComposeConfig> {
    let services = convert_services(compose, working_dir)?;
    let networks = convert_networks(compose, project_name);
    let volumes = convert_volumes(compose);

    Ok(ComposeConfig {
        services,
        networks,
        volumes,
    })
}

fn convert_services(
    compose: &Compose,
    working_dir: &Path,
) -> Result<IndexMap<String, ServiceConfig>> {
    let mut services = IndexMap::new();

    for (name, svc_opt) in compose.services.0.iter() {
        let svc = match svc_opt {
            Some(svc) => svc,
            None => continue,
        };

        let config = ServiceConfig {
            name: name.clone(),
            image: svc.image.clone(),
            container_name: svc.container_name.clone(),
            hostname: svc.hostname.clone(),
            command: convert_command(&svc.command),
            environment: convert_environment(&svc.environment),
            ports: convert_ports(&svc.ports)?,
            expose: svc.expose.clone(),
            volumes: convert_volumes_mounts(&svc.volumes, working_dir)?,
            networks: convert_service_networks(&svc.networks),
            restart: convert_restart_policy(svc.restart.as_deref()),
            logging: convert_logging(&svc.logging),
            labels: convert_labels(&svc.labels),
            depends_on: convert_depends_on(&svc.depends_on),
        };

        services.insert(name.clone(), config);
    }

    Ok(services)
}

/// Convert docker_compose_types Command to Vec<String>.
pub fn convert_command(cmd: &Option<Command>) -> Option<Vec<String>> {
    match cmd {
        None => None,
        Some(Command::Simple(s)) => {
            // Split shell-style command string
            match shell_words::split(s) {
                Ok(words) => Some(words),
                Err(_) => Some(vec![s.clone()]),
            }
        }
        Some(Command::Args(args)) => Some(args.clone()),
    }
}

/// Convert docker_compose_types Environment to HashMap.
pub fn convert_environment(env: &Environment) -> HashMap<String, Option<String>> {
    let mut result = HashMap::new();
    match env {
        Environment::List(list) => {
            for item in list {
                if let Some(pos) = item.find('=') {
                    let key = item[..pos].to_string();
                    let val = item[pos + 1..].to_string();
                    result.insert(key, Some(val));
                } else {
                    // Variable with no value - should be inherited from host
                    result.insert(item.clone(), None);
                }
            }
        }
        Environment::KvPair(map) => {
            for (key, val) in map.iter() {
                let value = val.as_ref().map(single_value_to_string);
                result.insert(key.clone(), value);
            }
        }
    }
    result
}

fn single_value_to_string(v: &SingleValue) -> String {
    match v {
        SingleValue::String(s) => s.clone(),
        SingleValue::Bool(b) => b.to_string(),
        SingleValue::Unsigned(n) => n.to_string(),
        SingleValue::Signed(n) => n.to_string(),
        SingleValue::Float(f) => f.to_string(),
    }
}

/// Convert docker_compose_types Ports to Vec<PortMapping>.
pub fn convert_ports(ports: &Ports) -> Result<Vec<PortMapping>> {
    match ports {
        Ports::Short(list) => list.iter().map(|s| parse_port_string(s)).collect(),
        Ports::Long(list) => list.iter().map(convert_long_port).collect(),
    }
}

/// Parse a short-form port string like "80:80", "127.0.0.1:8080:80/tcp", "443:443".
pub fn parse_port_string(s: &str) -> Result<PortMapping> {
    let (port_part, protocol) = if let Some(pos) = s.rfind('/') {
        (&s[..pos], s[pos + 1..].to_string())
    } else {
        (s, "tcp".to_string())
    };

    // Split by ':' to get [host_ip:]host_port:container_port or just container_port
    let parts: Vec<&str> = port_part.split(':').collect();

    match parts.len() {
        1 => {
            // Just container port
            let port = parse_port_number(parts[0])?;
            Ok(PortMapping {
                host_ip: String::new(),
                host_port: port,
                container_port: port,
                protocol,
            })
        }
        2 => {
            // host_port:container_port
            let host_port = parse_port_number(parts[0])?;
            let container_port = parse_port_number(parts[1])?;
            Ok(PortMapping {
                host_ip: String::new(),
                host_port,
                container_port,
                protocol,
            })
        }
        3 => {
            // host_ip:host_port:container_port
            let host_port = parse_port_number(parts[1])?;
            let container_port = parse_port_number(parts[2])?;
            Ok(PortMapping {
                host_ip: parts[0].to_string(),
                host_port,
                container_port,
                protocol,
            })
        }
        _ => Err(ComposeError::InvalidPort(format!(
            "invalid port format: {s}"
        ))),
    }
}

fn parse_port_number(s: &str) -> Result<u16> {
    s.parse::<u16>()
        .map_err(|_| ComposeError::InvalidPort(format!("invalid port number: {s}")))
}

fn convert_long_port(port: &Port) -> Result<PortMapping> {
    let host_port = match &port.published {
        Some(PublishedPort::Single(p)) => *p,
        Some(PublishedPort::Range(r)) => {
            // For ranges, use the start of the range
            r.split('-')
                .next()
                .and_then(|s| s.parse::<u16>().ok())
                .ok_or_else(|| ComposeError::InvalidPort(format!("invalid port range: {r}")))?
        }
        None => port.target,
    };

    Ok(PortMapping {
        host_ip: port.host_ip.clone().unwrap_or_default(),
        host_port,
        container_port: port.target,
        protocol: port.protocol.clone().unwrap_or_else(|| "tcp".to_string()),
    })
}

/// Convert docker_compose_types Volumes (service-level) to Vec<VolumeMount>.
pub fn convert_volumes_mounts(
    volumes: &[Volumes],
    working_dir: &Path,
) -> Result<Vec<VolumeMount>> {
    volumes
        .iter()
        .map(|v| convert_single_volume(v, working_dir))
        .collect()
}

fn convert_single_volume(volume: &Volumes, working_dir: &Path) -> Result<VolumeMount> {
    match volume {
        Volumes::Simple(s) => parse_volume_string(s, working_dir),
        Volumes::Advanced(adv) => convert_advanced_volume(adv, working_dir),
    }
}

/// Parse a short-form volume string like "./data:/app/data:ro", "/host:/container".
pub fn parse_volume_string(s: &str, working_dir: &Path) -> Result<VolumeMount> {
    let parts: Vec<&str> = s.split(':').collect();

    match parts.len() {
        1 => {
            // Anonymous volume
            Ok(VolumeMount {
                source: String::new(),
                target: parts[0].to_string(),
                read_only: false,
                mount_type: MountType::Volume,
            })
        }
        2 => {
            // source:target
            let source = resolve_volume_source(parts[0], working_dir);
            let mount_type = infer_mount_type(parts[0]);
            Ok(VolumeMount {
                source,
                target: parts[1].to_string(),
                read_only: false,
                mount_type,
            })
        }
        3 => {
            // source:target:mode
            let source = resolve_volume_source(parts[0], working_dir);
            let mount_type = infer_mount_type(parts[0]);
            let read_only = parts[2] == "ro";
            Ok(VolumeMount {
                source,
                target: parts[1].to_string(),
                read_only,
                mount_type,
            })
        }
        _ => Err(ComposeError::InvalidVolume(format!(
            "invalid volume format: {s}"
        ))),
    }
}

fn resolve_volume_source(source: &str, working_dir: &Path) -> String {
    if source.starts_with("./") || source.starts_with("../") {
        // Relative path - resolve against working directory
        working_dir
            .join(source)
            .to_string_lossy()
            .into_owned()
    } else {
        source.to_string()
    }
}

fn infer_mount_type(source: &str) -> MountType {
    if source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with('~')
    {
        MountType::Bind
    } else {
        MountType::Volume
    }
}

fn convert_advanced_volume(adv: &AdvancedVolumes, working_dir: &Path) -> Result<VolumeMount> {
    let mount_type = match adv._type.as_str() {
        "bind" => MountType::Bind,
        "tmpfs" => MountType::Tmpfs,
        _ => MountType::Volume,
    };

    let source = match &adv.source {
        Some(s) => resolve_volume_source(s, working_dir),
        None => String::new(),
    };

    Ok(VolumeMount {
        source,
        target: adv.target.clone(),
        read_only: adv.read_only,
        mount_type,
    })
}

/// Convert service-level networks to a list of network names.
pub fn convert_service_networks(networks: &Networks) -> Vec<String> {
    match networks {
        Networks::Simple(list) => list.clone(),
        Networks::Advanced(AdvancedNetworks(map)) => map.keys().cloned().collect(),
    }
}

/// Convert restart policy string to our enum.
pub fn convert_restart_policy(restart: Option<&str>) -> RestartPolicyConfig {
    match restart {
        Some(s) => RestartPolicyConfig::from_str_policy(s),
        None => RestartPolicyConfig::No,
    }
}

/// Convert logging configuration.
pub fn convert_logging(logging: &Option<LoggingParameters>) -> LoggingConfig {
    match logging {
        None => LoggingConfig::default(),
        Some(lp) => LoggingConfig {
            driver: lp.driver.clone(),
            options: lp
                .options
                .as_ref()
                .map(|opts| {
                    opts.iter()
                        .map(|(k, v)| (k.clone(), single_value_to_string(v)))
                        .collect()
                })
                .unwrap_or_default(),
        },
    }
}

/// Convert labels to HashMap.
pub fn convert_labels(labels: &Labels) -> HashMap<String, String> {
    match labels {
        Labels::List(list) => {
            let mut map = HashMap::new();
            for item in list {
                if let Some(pos) = item.find('=') {
                    map.insert(item[..pos].to_string(), item[pos + 1..].to_string());
                } else {
                    map.insert(item.clone(), String::new());
                }
            }
            map
        }
        Labels::Map(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

/// Convert depends_on to a list of service names.
pub fn convert_depends_on(deps: &DependsOnOptions) -> Vec<String> {
    match deps {
        DependsOnOptions::Simple(list) => list.clone(),
        DependsOnOptions::Conditional(map) => map.keys().cloned().collect(),
    }
}

/// Convert top-level networks to our NetworkConfig.
fn convert_networks(compose: &Compose, project_name: &str) -> IndexMap<String, NetworkConfig> {
    let mut networks = IndexMap::new();

    for (key, net_or_empty) in compose.networks.0.iter() {
        let config = match net_or_empty {
            MapOrEmpty::Map(settings) => convert_network_settings(key, settings, project_name),
            MapOrEmpty::Empty => NetworkConfig {
                key: key.clone(),
                name: NetworkConfig::resolve_name(key, None, project_name),
                driver: None,
                external: false,
                labels: HashMap::new(),
            },
        };
        networks.insert(key.clone(), config);
    }

    networks
}

fn convert_network_settings(
    key: &str,
    settings: &NetworkSettings,
    project_name: &str,
) -> NetworkConfig {
    let external = settings.external.is_some();
    let name = if external {
        // External networks use their key name or explicit name as-is
        settings
            .name
            .clone()
            .unwrap_or_else(|| key.to_string())
    } else {
        NetworkConfig::resolve_name(key, settings.name.as_deref(), project_name)
    };

    NetworkConfig {
        key: key.to_string(),
        name,
        driver: settings.driver.clone(),
        external,
        labels: convert_labels(&settings.labels),
    }
}

/// Convert top-level volumes.
fn convert_volumes(compose: &Compose) -> IndexMap<String, VolumeConfig> {
    let mut volumes = IndexMap::new();

    for (key, vol_or_empty) in compose.volumes.0.iter() {
        let config = match vol_or_empty {
            MapOrEmpty::Empty => VolumeConfig::default(),
            MapOrEmpty::Map(cv) => VolumeConfig {
                name: cv.name.clone(),
                driver: cv.driver.clone(),
                external: cv.external.is_some(),
            },
        };
        volumes.insert(key.clone(), config);
    }

    volumes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_command_simple() {
        let cmd = Some(Command::Simple("python app.py --debug".to_string()));
        let result = convert_command(&cmd).unwrap();
        assert_eq!(result, vec!["python", "app.py", "--debug"]);
    }

    #[test]
    fn test_convert_command_args() {
        let cmd = Some(Command::Args(vec![
            "python".to_string(),
            "app.py".to_string(),
        ]));
        let result = convert_command(&cmd).unwrap();
        assert_eq!(result, vec!["python", "app.py"]);
    }

    #[test]
    fn test_convert_command_none() {
        assert!(convert_command(&None).is_none());
    }

    #[test]
    fn test_convert_environment_list() {
        let env = Environment::List(vec![
            "FOO=bar".to_string(),
            "BAZ=".to_string(),
            "EMPTY".to_string(),
        ]);
        let result = convert_environment(&env);
        assert_eq!(result.get("FOO"), Some(&Some("bar".to_string())));
        assert_eq!(result.get("BAZ"), Some(&Some(String::new())));
        assert_eq!(result.get("EMPTY"), Some(&None));
    }

    #[test]
    fn test_convert_environment_kvpair() {
        let mut map = IndexMap::new();
        map.insert(
            "KEY".to_string(),
            Some(SingleValue::String("value".to_string())),
        );
        map.insert("EMPTY".to_string(), None);
        let env = Environment::KvPair(map);
        let result = convert_environment(&env);
        assert_eq!(result.get("KEY"), Some(&Some("value".to_string())));
        assert_eq!(result.get("EMPTY"), Some(&None));
    }

    #[test]
    fn test_parse_port_simple() {
        let pm = parse_port_string("80:80").unwrap();
        assert_eq!(pm.host_port, 80);
        assert_eq!(pm.container_port, 80);
        assert_eq!(pm.protocol, "tcp");
        assert!(pm.host_ip.is_empty());
    }

    #[test]
    fn test_parse_port_different() {
        let pm = parse_port_string("8080:80").unwrap();
        assert_eq!(pm.host_port, 8080);
        assert_eq!(pm.container_port, 80);
    }

    #[test]
    fn test_parse_port_with_ip() {
        let pm = parse_port_string("127.0.0.1:8080:80").unwrap();
        assert_eq!(pm.host_ip, "127.0.0.1");
        assert_eq!(pm.host_port, 8080);
        assert_eq!(pm.container_port, 80);
    }

    #[test]
    fn test_parse_port_with_protocol() {
        let pm = parse_port_string("53:53/udp").unwrap();
        assert_eq!(pm.protocol, "udp");
        assert_eq!(pm.container_port, 53);
    }

    #[test]
    fn test_parse_port_single() {
        let pm = parse_port_string("3000").unwrap();
        assert_eq!(pm.host_port, 3000);
        assert_eq!(pm.container_port, 3000);
    }

    #[test]
    fn test_parse_volume_bind_relative() {
        let vm = parse_volume_string("./data:/app/data", Path::new("/project")).unwrap();
        assert_eq!(vm.source, "/project/./data");
        assert_eq!(vm.target, "/app/data");
        assert!(!vm.read_only);
        assert_eq!(vm.mount_type, MountType::Bind);
    }

    #[test]
    fn test_parse_volume_bind_absolute() {
        let vm = parse_volume_string("/host/path:/container/path", Path::new("/project")).unwrap();
        assert_eq!(vm.source, "/host/path");
        assert_eq!(vm.target, "/container/path");
        assert_eq!(vm.mount_type, MountType::Bind);
    }

    #[test]
    fn test_parse_volume_readonly() {
        let vm =
            parse_volume_string("./config:/etc/config:ro", Path::new("/project")).unwrap();
        assert!(vm.read_only);
    }

    #[test]
    fn test_parse_volume_named() {
        let vm = parse_volume_string("myvolume:/data", Path::new("/project")).unwrap();
        assert_eq!(vm.source, "myvolume");
        assert_eq!(vm.mount_type, MountType::Volume);
    }

    #[test]
    fn test_convert_restart_policy() {
        assert_eq!(
            convert_restart_policy(Some("always")),
            RestartPolicyConfig::Always
        );
        assert_eq!(
            convert_restart_policy(Some("unless-stopped")),
            RestartPolicyConfig::UnlessStopped
        );
        assert_eq!(
            convert_restart_policy(Some("on-failure")),
            RestartPolicyConfig::OnFailure
        );
        assert_eq!(convert_restart_policy(None), RestartPolicyConfig::No);
    }

    #[test]
    fn test_convert_depends_on_simple() {
        let deps = DependsOnOptions::Simple(vec!["db".to_string(), "redis".to_string()]);
        let result = convert_depends_on(&deps);
        assert_eq!(result, vec!["db", "redis"]);
    }

    #[test]
    fn test_convert_service_networks_simple() {
        let networks = Networks::Simple(vec!["frontend".to_string(), "backend".to_string()]);
        let result = convert_service_networks(&networks);
        assert_eq!(result, vec!["frontend", "backend"]);
    }

    #[test]
    fn test_full_compose_conversion() {
        let yaml = r#"
services:
  web:
    image: nginx:latest
    container_name: my-web
    hostname: web-host
    command: "nginx -g 'daemon off;'"
    ports:
      - "80:80"
      - "443:443"
    environment:
      - FOO=bar
      - EMPTY=
    volumes:
      - ./html:/usr/share/nginx/html:ro
    networks:
      - frontend
    restart: unless-stopped
    logging:
      driver: json-file
      options:
        max-size: "10m"
  db:
    image: postgres:16
    expose:
      - "5432"
    environment:
      POSTGRES_PASSWORD: secret
    volumes:
      - pgdata:/var/lib/postgresql/data

networks:
  frontend:
    driver: bridge
    name: my-frontend

volumes:
  pgdata:
"#;
        let compose: Compose = serde_yaml::from_str(yaml).unwrap();
        let config = convert_compose(&compose, "myproject", Path::new("/work")).unwrap();

        // Services
        assert_eq!(config.services.len(), 2);
        let web = &config.services["web"];
        assert_eq!(web.image.as_deref(), Some("nginx:latest"));
        assert_eq!(web.container_name.as_deref(), Some("my-web"));
        assert_eq!(web.hostname.as_deref(), Some("web-host"));
        assert_eq!(web.ports.len(), 2);
        assert_eq!(web.ports[0].container_port, 80);
        assert_eq!(web.ports[1].container_port, 443);
        assert_eq!(
            web.environment.get("FOO"),
            Some(&Some("bar".to_string()))
        );
        assert_eq!(web.volumes.len(), 1);
        assert!(web.volumes[0].read_only);
        assert_eq!(web.networks, vec!["frontend"]);
        assert_eq!(web.restart, RestartPolicyConfig::UnlessStopped);
        assert_eq!(web.logging.driver.as_deref(), Some("json-file"));

        let db = &config.services["db"];
        assert_eq!(db.image.as_deref(), Some("postgres:16"));
        assert_eq!(db.expose, vec!["5432"]);
        assert_eq!(db.volumes[0].mount_type, MountType::Volume);
        assert_eq!(db.volumes[0].source, "pgdata");

        // Networks
        assert_eq!(config.networks.len(), 1);
        let frontend = &config.networks["frontend"];
        assert_eq!(frontend.driver.as_deref(), Some("bridge"));
        assert_eq!(frontend.name, "my-frontend");

        // Volumes
        assert_eq!(config.volumes.len(), 1);
        assert!(config.volumes.contains_key("pgdata"));
    }
}
