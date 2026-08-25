use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{ComposeError, Result};
use crate::model::ComposeConfig;

/// A directed acyclic graph of service dependencies.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Adjacency list: service → set of services it depends on.
    pub dependencies: HashMap<String, HashSet<String>>,
    /// Reverse adjacency: service → set of services that depend on it.
    pub dependents: HashMap<String, HashSet<String>>,
    /// All service names.
    pub services: Vec<String>,
}

impl DependencyGraph {
    /// Build a dependency graph from a compose config.
    pub fn from_config(config: &ComposeConfig) -> Result<Self> {
        let services: Vec<String> = config.services.keys().cloned().collect();
        let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();
        let mut dependents: HashMap<String, HashSet<String>> = HashMap::new();

        // Initialize all services
        for name in &services {
            dependencies.entry(name.clone()).or_default();
            dependents.entry(name.clone()).or_default();
        }

        // Add edges from depends_on
        for (name, svc) in &config.services {
            for dep in &svc.depends_on {
                if !config.services.contains_key(dep) {
                    return Err(ComposeError::ServiceNotFound(format!(
                        "service '{name}' depends on unknown service '{dep}'"
                    )));
                }
                dependencies
                    .entry(name.clone())
                    .or_default()
                    .insert(dep.clone());
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .insert(name.clone());
            }
        }

        let graph = Self {
            dependencies,
            dependents,
            services,
        };

        graph.detect_cycles()?;
        Ok(graph)
    }

    /// Detect cycles in the dependency graph. Returns an error with the cycle path.
    fn detect_cycles(&self) -> Result<()> {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        let mut path = Vec::new();

        for service in &self.services {
            if !visited.contains(service) {
                if let Some(cycle) = self.dfs_cycle(service, &mut visited, &mut stack, &mut path) {
                    return Err(ComposeError::CircularDependency(cycle.join(" -> ")));
                }
            }
        }
        Ok(())
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = self.dependencies.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if let Some(cycle) = self.dfs_cycle(dep, visited, stack, path) {
                        return Some(cycle);
                    }
                } else if stack.contains(dep) {
                    // Found a cycle — extract the cycle path
                    let start = path.iter().position(|s| s == dep).unwrap();
                    let mut cycle: Vec<String> = path[start..].to_vec();
                    cycle.push(dep.clone());
                    return Some(cycle);
                }
            }
        }

        stack.remove(node);
        path.pop();
        None
    }

    /// Return a topological ordering of services (startup order).
    /// Uses Kahn's algorithm.
    pub fn topological_sort(&self) -> Vec<String> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for svc in &self.services {
            in_degree.insert(svc.as_str(), 0);
        }

        // in_degree[svc] = number of services svc depends on
        // (svc must wait for all its dependencies before starting)
        for (svc, deps) in &self.dependencies {
            in_degree.insert(svc.as_str(), deps.len());
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        for (svc, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(svc.to_string());
            }
        }

        // Sort initial queue for deterministic output
        let mut initial: Vec<String> = queue.drain(..).collect();
        initial.sort();
        queue.extend(initial);

        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            // For each service that depends on `node`, decrease in_degree
            if let Some(deps) = self.dependents.get(&node) {
                let mut next: Vec<&String> = deps.iter().collect();
                next.sort(); // deterministic
                for dependent in next {
                    if let Some(degree) = in_degree.get_mut(dependent.as_str()) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }

        result
    }

    /// Return a reverse topological ordering (shutdown order).
    pub fn reverse_topological_sort(&self) -> Vec<String> {
        let mut result = self.topological_sort();
        result.reverse();
        result
    }

    /// Return groups of services that can be started concurrently.
    /// Each group's dependencies are satisfied by all previous groups.
    pub fn parallel_groups(&self) -> Vec<Vec<String>> {
        let mut groups = Vec::new();
        let mut remaining_deps: HashMap<String, HashSet<String>> = self.dependencies.clone();
        let mut started: HashSet<String> = HashSet::new();

        loop {
            // Find all services whose dependencies are all in `started`
            let mut group: Vec<String> = remaining_deps
                .iter()
                .filter(|(svc, deps)| !started.contains(*svc) && deps.is_subset(&started))
                .map(|(svc, _)| svc.clone())
                .collect();

            if group.is_empty() {
                break;
            }

            group.sort(); // deterministic ordering
            for svc in &group {
                started.insert(svc.clone());
                remaining_deps.remove(svc);
            }
            groups.push(group);
        }

        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::service::ServiceConfig;
    use crate::model::types::{LoggingConfig, RestartPolicyConfig};
    use indexmap::IndexMap;

    fn make_config(services: Vec<(&str, Vec<&str>)>) -> ComposeConfig {
        let mut svc_map = IndexMap::new();
        for (name, deps) in services {
            svc_map.insert(
                name.to_string(),
                ServiceConfig {
                    name: name.to_string(),
                    image: Some("alpine".to_string()),
                    container_name: None,
                    hostname: None,
                    command: None,
                    environment: HashMap::new(),
                    ports: vec![],
                    expose: vec![],
                    volumes: vec![],
                    networks: vec![],
                    restart: RestartPolicyConfig::No,
                    logging: LoggingConfig::default(),
                    labels: HashMap::new(),
                    depends_on: deps.iter().map(|s| s.to_string()).collect(),
                    healthcheck: None,
                },
            );
        }
        ComposeConfig {
            services: svc_map,
            networks: IndexMap::new(),
            volumes: IndexMap::new(),
        }
    }

    #[test]
    fn test_no_deps_single_group() {
        let config = make_config(vec![("a", vec![]), ("b", vec![]), ("c", vec![])]);
        let graph = DependencyGraph::from_config(&config).unwrap();

        let groups = graph.parallel_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec!["a", "b", "c"]);
    }

    #[test]
    fn test_linear_chain() {
        // A depends on B, B depends on C
        let config = make_config(vec![("a", vec!["b"]), ("b", vec!["c"]), ("c", vec![])]);
        let graph = DependencyGraph::from_config(&config).unwrap();

        let order = graph.topological_sort();
        assert_eq!(order, vec!["c", "b", "a"]);

        let shutdown = graph.reverse_topological_sort();
        assert_eq!(shutdown, vec!["a", "b", "c"]);

        let groups = graph.parallel_groups();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec!["c"]);
        assert_eq!(groups[1], vec!["b"]);
        assert_eq!(groups[2], vec!["a"]);
    }

    #[test]
    fn test_diamond_dependency() {
        // A depends on B and C, B depends on D, C depends on D
        let config = make_config(vec![
            ("a", vec!["b", "c"]),
            ("b", vec!["d"]),
            ("c", vec!["d"]),
            ("d", vec![]),
        ]);
        let graph = DependencyGraph::from_config(&config).unwrap();

        let groups = graph.parallel_groups();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec!["d"]);
        assert_eq!(groups[1], vec!["b", "c"]);
        assert_eq!(groups[2], vec!["a"]);
    }

    #[test]
    fn test_cycle_detection() {
        let config = make_config(vec![("a", vec!["b"]), ("b", vec!["c"]), ("c", vec!["a"])]);
        let result = DependencyGraph::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("circular dependency"), "got: {err}");
    }

    #[test]
    fn test_unknown_dependency() {
        let config = make_config(vec![("a", vec!["nonexistent"])]);
        let result = DependencyGraph::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"), "got: {err}");
    }
}
