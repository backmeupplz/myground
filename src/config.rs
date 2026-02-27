use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_storage_path: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ServiceState {
    pub installed: bool,
    #[serde(default)]
    pub env_overrides: HashMap<String, String>,
    #[serde(default)]
    pub storage_paths: HashMap<String, String>,
}

/// Resolve the myground data directory (default: ~/.myground).
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".myground")
}

/// Ensure the base data directory exists.
pub fn ensure_data_dir(base: &Path) -> Result<(), ServiceError> {
    std::fs::create_dir_all(base.join("services"))
        .map_err(|e| ServiceError::Io(format!("Failed to create data dir: {e}")))?;
    Ok(())
}

/// Read or create the global config.
pub fn load_global_config(base: &Path) -> Result<GlobalConfig, ServiceError> {
    let path = base.join("config.toml");
    if path.exists() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| ServiceError::Io(format!("Failed to read config: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| ServiceError::Io(format!("Failed to parse config: {e}")))
    } else {
        let config = GlobalConfig {
            version: env!("CARGO_PKG_VERSION").to_string(),
            default_storage_path: None,
        };
        save_global_config(base, &config)?;
        Ok(config)
    }
}

/// Write the global config.
pub fn save_global_config(base: &Path, config: &GlobalConfig) -> Result<(), ServiceError> {
    let path = base.join("config.toml");
    let contents =
        toml::to_string_pretty(config).map_err(|e| ServiceError::Io(format!("Serialize: {e}")))?;
    std::fs::write(&path, contents)
        .map_err(|e| ServiceError::Io(format!("Failed to write config: {e}")))?;
    Ok(())
}

/// Path to a service's directory.
pub fn service_dir(base: &Path, service_id: &str) -> PathBuf {
    base.join("services").join(service_id)
}

/// Read a service's state.
pub fn load_service_state(base: &Path, service_id: &str) -> Result<ServiceState, ServiceError> {
    let path = service_dir(base, service_id).join("state.toml");
    if path.exists() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| ServiceError::Io(format!("Failed to read service state: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| ServiceError::Io(format!("Failed to parse service state: {e}")))
    } else {
        Ok(ServiceState::default())
    }
}

/// Write a service's state.
pub fn save_service_state(
    base: &Path,
    service_id: &str,
    state: &ServiceState,
) -> Result<(), ServiceError> {
    let dir = service_dir(base, service_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| ServiceError::Io(format!("Failed to create service dir: {e}")))?;
    let path = dir.join("state.toml");
    let contents = toml::to_string_pretty(state)
        .map_err(|e| ServiceError::Io(format!("Serialize service state: {e}")))?;
    std::fs::write(&path, contents)
        .map_err(|e| ServiceError::Io(format!("Failed to write service state: {e}")))?;
    Ok(())
}

/// List all installed service IDs by scanning the services directory.
pub fn list_installed_services(base: &Path) -> Vec<String> {
    let services_dir = base.join("services");
    let Ok(entries) = std::fs::read_dir(services_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| e.path().join("state.toml").exists())
        .filter_map(|e| {
            let state_path = e.path().join("state.toml");
            let contents = std::fs::read_to_string(state_path).ok()?;
            let state: ServiceState = toml::from_str(&contents).ok()?;
            if state.installed {
                Some(e.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Resolve storage paths for a service. Priority:
/// 1. Explicit per-service override in ServiceState.storage_paths
/// 2. Global default_storage_path: {default_storage_path}/{service_id}/{name}/
/// 3. Fallback: {base}/services/{service_id}/volumes/{name}/
///
/// Returns a map of `STORAGE_{name}` → absolute path.
pub fn resolve_storage_paths(
    base: &Path,
    service_id: &str,
    def: &ServiceDefinition,
    global_config: &GlobalConfig,
    service_state: &ServiceState,
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for vol in &def.storage {
        let key = format!("STORAGE_{}", vol.name);
        let path = if let Some(override_path) = service_state.storage_paths.get(&vol.name) {
            override_path.clone()
        } else if let Some(ref global_base) = global_config.default_storage_path {
            format!("{global_base}/{service_id}/{}/", vol.name)
        } else {
            base.join("services")
                .join(service_id)
                .join("volumes")
                .join(&vol.name)
                .to_string_lossy()
                .to_string()
        };
        result.insert(key, path);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{dummy_service_def, dummy_storage_volumes};

    #[test]
    fn global_config_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let config = load_global_config(base).unwrap();
        assert_eq!(config.version, env!("CARGO_PKG_VERSION"));

        let config2 = load_global_config(base).unwrap();
        assert_eq!(config2.version, config.version);
    }

    #[test]
    fn service_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let state = ServiceState {
            installed: true,
            env_overrides: HashMap::from([("PORT".to_string(), "9090".to_string())]),
            storage_paths: HashMap::from([("data".to_string(), "/mnt/data".to_string())]),
        };
        save_service_state(base, "whoami", &state).unwrap();

        let loaded = load_service_state(base, "whoami").unwrap();
        assert!(loaded.installed);
        assert_eq!(loaded.env_overrides.get("PORT").unwrap(), "9090");
        assert_eq!(loaded.storage_paths.get("data").unwrap(), "/mnt/data");
    }

    #[test]
    fn list_installed_services_finds_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        assert!(list_installed_services(base).is_empty());

        let state = ServiceState {
            installed: true,
            env_overrides: HashMap::new(),
            storage_paths: HashMap::new(),
        };
        save_service_state(base, "whoami", &state).unwrap();

        let installed = list_installed_services(base);
        assert_eq!(installed, vec!["whoami"]);
    }

    #[test]
    fn list_installed_ignores_uninstalled() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let state = ServiceState {
            installed: false,
            env_overrides: HashMap::new(),
            storage_paths: HashMap::new(),
        };
        save_service_state(base, "old-service", &state).unwrap();

        assert!(list_installed_services(base).is_empty());
    }

    #[test]
    fn resolve_storage_fallback_path() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let def = dummy_service_def("test", "", HashMap::new(), dummy_storage_volumes());
        let global = GlobalConfig::default();
        let state = ServiceState::default();

        let paths = resolve_storage_paths(base, "filebrowser", &def, &global, &state);
        let data_path = paths.get("STORAGE_data").unwrap();
        assert!(data_path.contains("services/filebrowser/volumes/data"));
        let config_path = paths.get("STORAGE_config").unwrap();
        assert!(config_path.contains("services/filebrowser/volumes/config"));
    }

    #[test]
    fn resolve_storage_global_default() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let def = dummy_service_def("test", "", HashMap::new(), dummy_storage_volumes());
        let global = GlobalConfig {
            version: "0.1.0".to_string(),
            default_storage_path: Some("/mnt/data".to_string()),
        };
        let state = ServiceState::default();

        let paths = resolve_storage_paths(base, "filebrowser", &def, &global, &state);
        assert_eq!(
            paths.get("STORAGE_data").unwrap(),
            "/mnt/data/filebrowser/data/"
        );
        assert_eq!(
            paths.get("STORAGE_config").unwrap(),
            "/mnt/data/filebrowser/config/"
        );
    }

    #[test]
    fn resolve_storage_per_service_override() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let def = dummy_service_def("test", "", HashMap::new(), dummy_storage_volumes());
        let global = GlobalConfig {
            version: "0.1.0".to_string(),
            default_storage_path: Some("/mnt/data".to_string()),
        };
        let state = ServiceState {
            installed: true,
            env_overrides: HashMap::new(),
            storage_paths: HashMap::from([("data".to_string(), "/mnt/photos".to_string())]),
        };

        let paths = resolve_storage_paths(base, "filebrowser", &def, &global, &state);
        assert_eq!(paths.get("STORAGE_data").unwrap(), "/mnt/photos");
        assert_eq!(
            paths.get("STORAGE_config").unwrap(),
            "/mnt/data/filebrowser/config/"
        );
    }
}
