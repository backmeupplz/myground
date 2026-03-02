use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use rand::Rng;

use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_access_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_secret_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiKeyEntry {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub created_at: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthConfig {
    pub username: String,
    pub password_hash: String,
    /// Hash of the CLI session token (if one is active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_token_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_keys: Vec<ApiKeyEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct TailscaleConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Auth key — read from old configs for migration, but never written back.
    #[serde(default, skip_serializing)]
    pub auth_key: Option<String>,
    /// User's tailnet name (e.g. "tail1234b.ts.net"), auto-detected after first start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailnet: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateConfig {
    #[serde(default)]
    pub auto_update_services: bool,
    #[serde(default)]
    pub auto_update_myground: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_myground_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_myground_url: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct CloudflareConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_token: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct DomainBinding {
    pub subdomain: String,
    pub zone_id: String,
    pub zone_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_record_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct GlobalConfig {
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_storage_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<BackupConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<UpdateConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloudflare: Option<CloudflareConfig>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceBackupConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<BackupConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<BackupConfig>,
    /// Backup schedule: "daily", "weekly", "monthly", or a 5-field cron expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ServiceState {
    pub installed: bool,
    #[serde(default)]
    pub env_overrides: HashMap<String, String>,
    #[serde(default)]
    pub storage_paths: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<ServiceBackupConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_password: Option<String>,
    /// ISO 8601 timestamp of the last successful scheduled backup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_backup_at: Option<String>,
    /// When true, Tailscale sidecar is not injected for this service.
    #[serde(default)]
    pub tailscale_disabled: bool,
    /// Custom Tailscale hostname for this service (default: myground-{id}).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale_hostname: Option<String>,
    /// When true, the service binds to 0.0.0.0 instead of 127.0.0.1 for LAN access.
    #[serde(default)]
    pub lan_accessible: bool,
    /// GPU acceleration mode: "nvidia" or "intel". None = disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_mode: Option<String>,
    /// Pinned Docker image digest (sha256) recorded at install/update time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    /// True when a newer Docker image has been detected.
    #[serde(default)]
    pub update_available: bool,
    /// ISO 8601 timestamp of the last update check for this service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<String>,
    /// Cloudflare domain binding for this service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<DomainBinding>,
}

// ── Generic TOML helpers ────────────────────────────────────────────────────

fn load_toml<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, ServiceError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ServiceError::Io(format!("Failed to read {label}: {e}")))?;
    toml::from_str(&contents)
        .map_err(|e| ServiceError::Io(format!("Failed to parse {label}: {e}")))
}

fn save_toml<T: Serialize>(path: &Path, value: &T, label: &str) -> Result<(), ServiceError> {
    let contents =
        toml::to_string_pretty(value).map_err(|e| ServiceError::Io(format!("Serialize {label}: {e}")))?;
    std::fs::write(path, contents)
        .map_err(|e| ServiceError::Io(format!("Failed to write {label}: {e}")))?;
    // Restrict file permissions to owner-only (contains secrets)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ── Service ID validation ────────────────────────────────────────────────────

/// Validate that a service ID is safe for use in filesystem paths.
/// Rejects IDs containing path traversal characters, null bytes, or other unsafe chars.
pub fn validate_service_id(id: &str) -> Result<(), ServiceError> {
    if id.is_empty() {
        return Err(ServiceError::Io("Service ID must not be empty".into()));
    }
    if id.len() > 128 {
        return Err(ServiceError::Io("Service ID too long".into()));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ServiceError::Io(format!(
            "Invalid service ID '{id}': must contain only a-z, A-Z, 0-9, '-', '_'"
        )));
    }
    if id.starts_with('-') || id.starts_with('_') {
        return Err(ServiceError::Io(format!(
            "Invalid service ID '{id}': must not start with '-' or '_'"
        )));
    }
    Ok(())
}

// ── Data directory ──────────────────────────────────────────────────────────

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

// ── Password generation ─────────────────────────────────────────────────────

/// Generate a random alphanumeric password of the given length.
pub fn generate_backup_password(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

/// Generate an 8-character hex ID for an API key.
pub fn generate_key_id() -> String {
    let mut rng = rand::rng();
    format!("{:08x}", rng.random::<u32>())
}

// ── Global config ───────────────────────────────────────────────────────────

/// Read or create the global config.
pub fn load_global_config(base: &Path) -> Result<GlobalConfig, ServiceError> {
    let path = base.join("config.toml");
    if path.exists() {
        load_toml(&path, "config")
    } else {
        let config = GlobalConfig {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        };
        save_global_config(base, &config)?;
        Ok(config)
    }
}

/// Write the global config.
pub fn save_global_config(base: &Path, config: &GlobalConfig) -> Result<(), ServiceError> {
    save_toml(&base.join("config.toml"), config, "config")
}

// ── Config accessors (generated by macro) ───────────────────────────────────

/// Generate load/save/try_load functions for a GlobalConfig field.
macro_rules! config_accessor {
    // Variant with try_load returning the config type (with Default fallback)
    ($field:ident, $type:ty, $load:ident, $save:ident, try_load = $try_load:ident) => {
        pub fn $load(base: &Path) -> Result<Option<$type>, ServiceError> {
            Ok(load_global_config(base)?.$field)
        }
        pub fn $save(base: &Path, value: &$type) -> Result<(), ServiceError> {
            let mut global = load_global_config(base)?;
            global.$field = Some(value.clone());
            save_global_config(base, &global)
        }
        pub fn $try_load(base: &Path) -> $type {
            $load(base).unwrap_or(None).unwrap_or_default()
        }
    };
    // Variant without try_load
    ($field:ident, $type:ty, $load:ident, $save:ident) => {
        pub fn $load(base: &Path) -> Result<Option<$type>, ServiceError> {
            Ok(load_global_config(base)?.$field)
        }
        pub fn $save(base: &Path, value: &$type) -> Result<(), ServiceError> {
            let mut global = load_global_config(base)?;
            global.$field = Some(value.clone());
            save_global_config(base, &global)
        }
    };
}

config_accessor!(auth, AuthConfig, load_auth_config, save_auth_config);
config_accessor!(tailscale, TailscaleConfig, load_tailscale_config, save_tailscale_config, try_load = try_load_tailscale);
config_accessor!(cloudflare, CloudflareConfig, load_cloudflare_config, save_cloudflare_config, try_load = try_load_cloudflare);
config_accessor!(backup, BackupConfig, load_backup_config, save_backup_config);

/// Load auth config, returning None on both missing and error.
pub fn try_load_auth(base: &Path) -> Option<AuthConfig> {
    load_auth_config(base).unwrap_or(None)
}

// ── Service state ───────────────────────────────────────────────────────────

/// Path to a service's directory.
pub fn service_dir(base: &Path, service_id: &str) -> PathBuf {
    base.join("services").join(service_id)
}

/// Read a service's state.
pub fn load_service_state(base: &Path, service_id: &str) -> Result<ServiceState, ServiceError> {
    let path = service_dir(base, service_id).join("state.toml");
    if path.exists() {
        load_toml(&path, "service state")
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
    save_toml(&dir.join("state.toml"), state, "service state")
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

// ── Storage path validation ──────────────────────────────────────────────────

/// Validate that a storage path does not traverse to sensitive system directories.
pub fn validate_storage_path(path: &str) -> Result<(), ServiceError> {
    let p = std::path::Path::new(path);
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(ServiceError::Io(
                "Storage path must not contain '..'".into(),
            ));
        }
    }
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let s = canonical.to_string_lossy();
    const BLOCKED: &[&str] = &[
        "/proc",
        "/sys",
        "/dev",
        "/run",
        "/boot",
        "/etc",
        "/root",
        "/var/run",
        "/tmp",
        "/var/lib/docker",
    ];
    for blocked in BLOCKED {
        if s.starts_with(blocked) {
            return Err(ServiceError::Io(format!(
                "Storage path must not be under {blocked}"
            )));
        }
    }
    Ok(())
}

// ── Storage path resolution ─────────────────────────────────────────────────

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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
        assert!(paths.get("STORAGE_data").unwrap().contains("services/filebrowser/volumes/data"));
        assert!(paths.get("STORAGE_config").unwrap().contains("services/filebrowser/volumes/config"));
    }

    #[test]
    fn resolve_storage_global_default() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let def = dummy_service_def("test", "", HashMap::new(), dummy_storage_volumes());
        let global = GlobalConfig {
            default_storage_path: Some("/mnt/data".to_string()),
            ..Default::default()
        };
        let state = ServiceState::default();

        let paths = resolve_storage_paths(base, "filebrowser", &def, &global, &state);
        assert_eq!(paths.get("STORAGE_data").unwrap(), "/mnt/data/filebrowser/data/");
        assert_eq!(paths.get("STORAGE_config").unwrap(), "/mnt/data/filebrowser/config/");
    }

    #[test]
    fn global_config_with_backup_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let backup = BackupConfig {
            repository: Some("/backups".to_string()),
            password: Some("secret".to_string()),
            ..Default::default()
        };
        save_backup_config(base, &backup).unwrap();

        let loaded = load_global_config(base).unwrap();
        let loaded_backup = loaded.backup.unwrap();
        assert_eq!(loaded_backup.repository.unwrap(), "/backups");
        assert_eq!(loaded_backup.password.unwrap(), "secret");
    }

    #[test]
    fn backup_config_defaults_are_sensible() {
        let config = BackupConfig::default();
        assert!(config.repository.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn load_backup_config_returns_none_when_not_set() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let config = GlobalConfig {
            version: "0.1.0".to_string(),
            ..Default::default()
        };
        save_global_config(base, &config).unwrap();

        assert!(load_backup_config(base).unwrap().is_none());
    }

    #[test]
    fn resolve_storage_per_service_override() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let def = dummy_service_def("test", "", HashMap::new(), dummy_storage_volumes());
        let global = GlobalConfig {
            default_storage_path: Some("/mnt/data".to_string()),
            ..Default::default()
        };
        let state = ServiceState {
            installed: true,
            storage_paths: HashMap::from([("data".to_string(), "/mnt/photos".to_string())]),
            ..Default::default()
        };

        let paths = resolve_storage_paths(base, "filebrowser", &def, &global, &state);
        assert_eq!(paths.get("STORAGE_data").unwrap(), "/mnt/photos");
        assert_eq!(paths.get("STORAGE_config").unwrap(), "/mnt/data/filebrowser/config/");
    }

    #[test]
    fn service_state_with_port_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let state = ServiceState {
            installed: true,
            port: Some(9042),
            definition_id: Some("filebrowser".to_string()),
            ..Default::default()
        };
        save_service_state(base, "filebrowser-2", &state).unwrap();

        let loaded = load_service_state(base, "filebrowser-2").unwrap();
        assert_eq!(loaded.port, Some(9042));
        assert_eq!(loaded.definition_id.as_deref(), Some("filebrowser"));
    }

    #[test]
    fn service_state_with_backup_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let state = ServiceState {
            installed: true,
            backup: Some(ServiceBackupConfig {
                enabled: true,
                local: Some(BackupConfig {
                    repository: Some("/backups".to_string()),
                    password: Some("secret".to_string()),
                    ..Default::default()
                }),
                remote: None,
                schedule: Some("daily".to_string()),
            }),
            ..Default::default()
        };
        save_service_state(base, "whoami", &state).unwrap();

        let loaded = load_service_state(base, "whoami").unwrap();
        let backup = loaded.backup.unwrap();
        assert!(backup.enabled);
        assert_eq!(backup.local.unwrap().repository.unwrap(), "/backups");
        assert!(backup.remote.is_none());
    }

    #[test]
    fn generate_backup_password_correct_length() {
        let pwd = generate_backup_password(32);
        assert_eq!(pwd.len(), 32);

        let pwd2 = generate_backup_password(16);
        assert_eq!(pwd2.len(), 16);
    }

    #[test]
    fn generate_backup_password_is_alphanumeric() {
        let pwd = generate_backup_password(100);
        assert!(pwd.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn generate_backup_password_is_unique() {
        let a = generate_backup_password(32);
        let b = generate_backup_password(32);
        assert_ne!(a, b);
    }

    #[test]
    fn service_backup_config_defaults() {
        let config = ServiceBackupConfig::default();
        assert!(!config.enabled);
        assert!(config.local.is_none());
        assert!(config.remote.is_none());
    }

    #[test]
    fn generate_key_id_is_8_hex_chars() {
        let id = generate_key_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_key_id_is_unique() {
        let a = generate_key_id();
        let b = generate_key_id();
        assert_ne!(a, b);
    }

    #[test]
    fn auth_config_with_api_keys_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        let auth = AuthConfig {
            username: "admin".to_string(),
            password_hash: "hash".to_string(),
            cli_token_hash: None,
            api_keys: vec![ApiKeyEntry {
                id: "aabbccdd".to_string(),
                name: "test-key".to_string(),
                key_hash: "somehash".to_string(),
                created_at: "2026-03-01T00:00:00Z".to_string(),
            }],
        };
        save_auth_config(base, &auth).unwrap();

        let loaded = load_auth_config(base).unwrap().unwrap();
        assert_eq!(loaded.api_keys.len(), 1);
        assert_eq!(loaded.api_keys[0].id, "aabbccdd");
        assert_eq!(loaded.api_keys[0].name, "test-key");
    }

    #[test]
    fn auth_config_backward_compat_no_api_keys() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        ensure_data_dir(base).unwrap();

        // Save config without api_keys field (simulates old config)
        let auth = AuthConfig {
            username: "admin".to_string(),
            password_hash: "hash".to_string(),
            cli_token_hash: None,
            api_keys: vec![],
        };
        save_auth_config(base, &auth).unwrap();

        let loaded = load_auth_config(base).unwrap().unwrap();
        assert!(loaded.api_keys.is_empty());
    }
}
