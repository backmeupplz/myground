use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use crate::config::{self, ServiceState};
use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

/// Result returned after a successful install.
pub struct InstallResult {
    pub instance_id: String,
    pub port: u16,
}

pub const PORT_RANGE_START: u16 = 9000;
pub const PORT_RANGE_END: u16 = 9999;

/// Collect all ports already in use by installed services and registry defaults.
pub fn used_ports(base: &Path, registry: &HashMap<String, ServiceDefinition>) -> HashSet<u16> {
    let mut ports = HashSet::new();

    // Collect registry default ports
    for def in registry.values() {
        for val in def.defaults.values() {
            if let Ok(p) = val.parse::<u16>() {
                ports.insert(p);
            }
        }
    }

    // Collect ports from installed services
    for id in config::list_installed_services(base) {
        if let Ok(state) = config::load_service_state(base, &id) {
            if let Some(p) = state.port {
                ports.insert(p);
            }
            for val in state.env_overrides.values() {
                if let Ok(p) = val.parse::<u16>() {
                    ports.insert(p);
                }
            }
        }
    }

    ports
}

/// Allocate the next free port in 9000-9999.
pub fn allocate_port(base: &Path, registry: &HashMap<String, ServiceDefinition>) -> Result<u16, ServiceError> {
    let in_use = used_ports(base, registry);
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if !in_use.contains(&port) {
            return Ok(port);
        }
    }
    Err(ServiceError::Io("No free ports in range 9000-9999".to_string()))
}

/// Generate the next instance ID for a multi-instance service.
pub fn next_instance_id(base: &Path, base_id: &str) -> String {
    let installed = config::list_installed_services(base);
    if !installed.contains(&base_id.to_string()) {
        return base_id.to_string();
    }
    for n in 2u32.. {
        let candidate = format!("{base_id}-{n}");
        if !installed.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Look up a service definition by ID: check registry first, then check if
/// the service state has a `definition_id` pointing to a parent template.
pub fn lookup_definition<'a>(
    service_id: &str,
    registry: &'a HashMap<String, ServiceDefinition>,
    base: &Path,
) -> Result<&'a ServiceDefinition, ServiceError> {
    if let Some(def) = registry.get(service_id) {
        return Ok(def);
    }
    let state = config::load_service_state(base, service_id).unwrap_or_default();
    if let Some(ref parent_id) = state.definition_id {
        if let Some(def) = registry.get(parent_id) {
            return Ok(def);
        }
    }
    Err(ServiceError::NotFound(service_id.to_string()))
}

/// Detect whether `docker compose` (v2) or `docker-compose` (v1) is available.
pub async fn detect_compose_command() -> Result<Vec<String>, ServiceError> {
    // Try v2 first
    let v2 = tokio::process::Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if let Ok(status) = v2 {
        if status.success() {
            return Ok(vec!["docker".to_string(), "compose".to_string()]);
        }
    }

    // Try v1
    let v1 = tokio::process::Command::new("docker-compose")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if let Ok(status) = v1 {
        if status.success() {
            return Ok(vec!["docker-compose".to_string()]);
        }
    }

    Err(ServiceError::Compose(
        "Neither 'docker compose' nor 'docker-compose' found".to_string(),
    ))
}

/// Generate docker-compose.yml content from a service definition and env vars.
pub fn generate_compose(def: &ServiceDefinition, env: &HashMap<String, String>) -> String {
    let mut result = def.compose_template.clone();
    for (key, value) in env {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}

/// Generate .env file content from defaults merged with overrides.
pub fn generate_env_file(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> String {
    let mut merged = defaults.clone();
    for (k, v) in overrides {
        merged.insert(k.clone(), v.clone());
    }
    let mut lines: Vec<String> = merged.iter().map(|(k, v)| format!("{k}={v}")).collect();
    lines.sort();
    lines.join("\n") + "\n"
}

/// Merge defaults with user overrides.
pub fn merge_env(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = defaults.clone();
    for (k, v) in overrides {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Install a service with full multi-instance + port allocation support.
///
/// If the service supports multi-instance and is already installed, a new
/// instance ID is generated (e.g. filebrowser-2). A port is auto-allocated.
/// If `storage_path` is provided, storage volumes are placed under it.
pub async fn install_service(
    base: &Path,
    registry: &HashMap<String, ServiceDefinition>,
    service_id: &str,
    storage_path: Option<&str>,
) -> Result<InstallResult, ServiceError> {
    let def = registry
        .get(service_id)
        .ok_or_else(|| ServiceError::NotFound(service_id.to_string()))?;

    // Determine instance ID (multi-instance support)
    let instance_id = if def.metadata.multi_instance {
        let existing = config::load_service_state(base, service_id);
        if existing.is_ok() && existing.unwrap().installed {
            next_instance_id(base, service_id)
        } else {
            service_id.to_string()
        }
    } else {
        let existing = config::load_service_state(base, service_id)?;
        if existing.installed {
            return Err(ServiceError::AlreadyInstalled(service_id.to_string()));
        }
        service_id.to_string()
    };

    // Allocate port
    let port = allocate_port(base, registry)?;

    // Build env overrides with allocated port
    let mut env_overrides = HashMap::new();
    for key in def.defaults.keys() {
        if key.ends_with("_PORT") {
            env_overrides.insert(key.clone(), port.to_string());
        }
    }

    // Build storage path overrides
    let mut storage_overrides = HashMap::new();
    if let Some(sp) = storage_path {
        for vol in &def.storage {
            storage_overrides.insert(
                vol.name.clone(),
                format!("{sp}/myground/{instance_id}/{}/", vol.name),
            );
        }
    }

    let global_config = config::load_global_config(base).unwrap_or_default();

    // Resolve storage paths
    let pre_state = ServiceState {
        storage_paths: storage_overrides.clone(),
        ..Default::default()
    };
    let storage_env =
        config::resolve_storage_paths(base, &instance_id, def, &global_config, &pre_state);

    // Create storage directories
    for path in storage_env.values() {
        std::fs::create_dir_all(path)
            .map_err(|e| ServiceError::Io(format!("Create storage dir: {e}")))?;
    }

    // Merge env: defaults + port overrides + storage env vars
    let mut merged_env = merge_env(&def.defaults, &env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    // For multi-instance, adjust container names in compose template
    let compose_template = if instance_id != service_id {
        def.compose_template.replace(
            &format!("myground-{service_id}"),
            &format!("myground-{instance_id}"),
        )
    } else {
        def.compose_template.clone()
    };

    let adjusted_def = ServiceDefinition {
        compose_template,
        ..def.clone()
    };

    // Write compose + .env
    let svc_dir = config::service_dir(base, &instance_id);
    std::fs::create_dir_all(&svc_dir)
        .map_err(|e| ServiceError::Io(format!("Create service dir: {e}")))?;

    let compose_content = generate_compose(&adjusted_def, &merged_env);
    std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content)
        .map_err(|e| ServiceError::Io(format!("Write compose file: {e}")))?;

    let mut env_with_storage = env_overrides.clone();
    for (k, v) in &storage_env {
        env_with_storage.insert(k.clone(), v.clone());
    }
    let env_content = generate_env_file(&def.defaults, &env_with_storage);
    std::fs::write(svc_dir.join(".env"), &env_content)
        .map_err(|e| ServiceError::Io(format!("Write .env: {e}")))?;

    // Build state storage_paths (vol name → resolved path)
    let mut state_storage_paths = storage_overrides;
    for vol in &def.storage {
        state_storage_paths
            .entry(vol.name.clone())
            .or_insert_with(|| {
                storage_env
                    .get(&format!("STORAGE_{}", vol.name))
                    .cloned()
                    .unwrap_or_default()
            });
    }

    // Save state
    let state = ServiceState {
        installed: true,
        env_overrides,
        storage_paths: state_storage_paths,
        port: Some(port),
        definition_id: if instance_id != service_id {
            Some(service_id.to_string())
        } else {
            None
        },
        backup: None,
    };
    config::save_service_state(base, &instance_id, &state)?;

    // Pull images and start
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["pull"]).await?;
    run_compose(&compose_cmd, &svc_dir, &["up", "-d"]).await?;

    Ok(InstallResult {
        instance_id,
        port,
    })
}

/// Start a service.
pub async fn start_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Stop a service.
pub async fn stop_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["down"]).await?;

    Ok(())
}

/// Remove a service: compose down, delete service metadata directory.
/// Does NOT delete user data in storage paths — user data is sacred.
pub async fn remove_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;

    // Try to bring down containers; ignore errors (may already be stopped)
    let _ = run_compose(&compose_cmd, &svc_dir, &["down", "--remove-orphans"]).await;

    // Warn about external storage paths that will be left intact
    for (name, path) in &state.storage_paths {
        if !path.starts_with(&svc_dir.to_string_lossy().to_string()) {
            tracing::info!("Keeping storage data for '{name}' at: {path}");
        }
    }

    // Remove the service metadata directory
    std::fs::remove_dir_all(&svc_dir)
        .map_err(|e| ServiceError::Io(format!("Remove service dir: {e}")))?;

    Ok(())
}

/// Nuke everything: stop all services, remove all containers, delete all data.
/// Returns a list of actions taken for display.
pub async fn nuke_all(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    // 1. Compose down for each installed service
    let installed = config::list_installed_services(base);
    if let Ok(compose_cmd) = detect_compose_command().await {
        for id in &installed {
            let svc_dir = config::service_dir(base, id);
            if svc_dir.join("docker-compose.yml").exists() {
                let result =
                    run_compose(&compose_cmd, &svc_dir, &["down", "--remove-orphans", "--volumes"])
                        .await;
                match result {
                    Ok(_) => actions.push(format!("Stopped and removed containers for {id}")),
                    Err(e) => actions.push(format!("Warning: compose down for {id}: {e}")),
                }
            }
        }
    }

    // 2. Force-remove any straggling myground-* containers
    if let Ok(output) = tokio::process::Command::new("docker")
        .args(["ps", "-a", "--filter", "name=myground-", "--format", "{{.Names}}"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        let names = String::from_utf8_lossy(&output.stdout);
        for name in names.lines().filter(|n| !n.is_empty()) {
            let _ = tokio::process::Command::new("docker")
                .args(["rm", "-f", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .await;
            actions.push(format!("Removed straggling container: {name}"));
        }
    }

    // 3. Remove the entire data directory
    if base.exists() {
        match std::fs::remove_dir_all(base) {
            Ok(()) => actions.push(format!("Removed data directory: {}", base.display())),
            Err(e) => actions.push(format!(
                "Warning: failed to remove {}: {e}",
                base.display()
            )),
        }
    }

    actions
}

/// Run a docker compose command in a service directory.
pub(crate) async fn run_compose(
    compose_cmd: &[String],
    work_dir: &Path,
    args: &[&str],
) -> Result<String, ServiceError> {
    let (program, base_args) = compose_cmd.split_first().expect("compose_cmd is non-empty");

    let output = tokio::process::Command::new(program)
        .args(base_args)
        .args(args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ServiceError::Compose(format!("Failed to run compose: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ServiceError::Compose(format!(
            "Compose command failed: {stderr}"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{dummy_service_def, dummy_storage_volumes};

    #[test]
    fn used_ports_returns_empty_when_no_services() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let registry = HashMap::new();
        assert!(used_ports(base, &registry).is_empty());
    }

    #[test]
    fn used_ports_includes_registry_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_service_def(
            "whoami",
            "",
            HashMap::from([("WHOAMI_PORT".to_string(), "8081".to_string())]),
            Vec::new(),
        );
        registry.insert("whoami".to_string(), def);

        let ports = used_ports(base, &registry);
        assert!(ports.contains(&8081));
    }

    #[test]
    fn allocate_port_returns_first_free_in_range() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let registry = HashMap::new();

        let port = allocate_port(base, &registry).unwrap();
        assert_eq!(port, PORT_RANGE_START);
    }

    #[test]
    fn allocate_port_skips_used_ports() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_service_def(
            "test",
            "",
            HashMap::from([("TEST_PORT".to_string(), "9000".to_string())]),
            Vec::new(),
        );
        registry.insert("test".to_string(), def);

        let port = allocate_port(base, &registry).unwrap();
        assert_eq!(port, 9001);
    }

    #[test]
    fn next_instance_id_returns_base_when_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        assert_eq!(next_instance_id(base, "filebrowser"), "filebrowser");
    }

    #[test]
    fn next_instance_id_returns_dash_2_when_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::ServiceState {
            installed: true,
            ..Default::default()
        };
        config::save_service_state(base, "filebrowser", &state).unwrap();

        assert_eq!(next_instance_id(base, "filebrowser"), "filebrowser-2");
    }

    #[test]
    fn generate_compose_substitutes_vars() {
        let def = dummy_service_def(
            "test",
            "ports:\n  - \"${PORT}:80\"",
            HashMap::from([("PORT".to_string(), "8080".to_string())]),
            Vec::new(),
        );

        let env = HashMap::from([("PORT".to_string(), "9090".to_string())]);
        let result = generate_compose(&def, &env);
        assert_eq!(result, "ports:\n  - \"9090:80\"");
    }

    #[test]
    fn generate_env_file_merges_correctly() {
        let defaults = HashMap::from([
            ("PORT".to_string(), "8080".to_string()),
            ("HOST".to_string(), "localhost".to_string()),
        ]);
        let overrides = HashMap::from([("PORT".to_string(), "9090".to_string())]);

        let result = generate_env_file(&defaults, &overrides);
        assert!(result.contains("PORT=9090"));
        assert!(result.contains("HOST=localhost"));
        assert!(!result.contains("PORT=8080"));
    }

    #[test]
    fn merge_env_applies_overrides() {
        let defaults = HashMap::from([("A".to_string(), "1".to_string())]);
        let overrides = HashMap::from([
            ("A".to_string(), "2".to_string()),
            ("B".to_string(), "3".to_string()),
        ]);
        let merged = merge_env(&defaults, &overrides);
        assert_eq!(merged.get("A").unwrap(), "2");
        assert_eq!(merged.get("B").unwrap(), "3");
    }

    #[test]
    fn generate_compose_with_storage_vars() {
        let def = dummy_service_def(
            "fb",
            "volumes:\n  - ${STORAGE_data}:/srv\n  - ${STORAGE_config}:/config",
            HashMap::new(),
            dummy_storage_volumes(),
        );

        let env = HashMap::from([
            ("STORAGE_data".to_string(), "/mnt/data/fb/data".to_string()),
            (
                "STORAGE_config".to_string(),
                "/mnt/data/fb/config".to_string(),
            ),
        ]);

        let result = generate_compose(&def, &env);
        assert!(result.contains("/mnt/data/fb/data:/srv"));
        assert!(result.contains("/mnt/data/fb/config:/config"));
        assert!(!result.contains("${STORAGE_"));
    }
}
