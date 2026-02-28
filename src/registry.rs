use std::collections::HashMap;

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(RustEmbed)]
#[folder = "services"]
struct ServiceFiles;

#[derive(Debug, Clone, Deserialize)]
struct RawServiceDefinition {
    metadata: ServiceMetadata,
    defaults: Option<HashMap<String, String>>,
    health: Option<HealthConfig>,
    storage: Option<StorageConfig>,
    install_variables: Option<Vec<InstallVariable>>,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageConfig {
    volumes: Vec<StorageVolume>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceMetadata {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub website: String,
    pub category: String,
    #[serde(default)]
    pub multi_instance: bool,
    #[serde(default = "default_true")]
    pub backup_supported: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthConfig {
    pub port: u16,
    pub path: String,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DbDumpConfig {
    pub container: String,
    pub command: String,
    pub dump_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InstallVariable {
    pub key: String,
    pub label: String,
    pub input_type: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StorageVolume {
    pub name: String,
    pub container_path: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_dump: Option<DbDumpConfig>,
}

#[derive(Debug, Clone)]
pub struct ServiceDefinition {
    pub metadata: ServiceMetadata,
    pub compose_template: String,
    pub defaults: HashMap<String, String>,
    pub health: Option<HealthConfig>,
    pub storage: Vec<StorageVolume>,
    pub install_variables: Vec<InstallVariable>,
}

/// Load all embedded service definitions by auto-discovering TOML files in `services/`.
pub fn load_registry() -> HashMap<String, ServiceDefinition> {
    let mut registry = HashMap::new();

    for filename in ServiceFiles::iter() {
        let filename_str = filename.as_ref();
        if !filename_str.ends_with(".toml") {
            continue;
        }
        let id = filename_str.trim_end_matches(".toml");
        let data = ServiceFiles::get(filename_str)
            .unwrap_or_else(|| panic!("Failed to read embedded file {filename_str}"));
        let toml_str = std::str::from_utf8(data.data.as_ref())
            .unwrap_or_else(|e| panic!("Invalid UTF-8 in {filename_str}: {e}"));

        let mut raw: RawServiceDefinition = toml::from_str(toml_str)
            .unwrap_or_else(|e| panic!("Failed to parse {filename_str}: {e}"));
        raw.metadata.id = id.to_string();

        let yml_filename = format!("{id}.yml");
        let yml_data = ServiceFiles::get(&yml_filename)
            .unwrap_or_else(|| panic!("Missing compose file {yml_filename}"));
        let compose_template = std::str::from_utf8(yml_data.data.as_ref())
            .unwrap_or_else(|e| panic!("Invalid UTF-8 in {yml_filename}: {e}"))
            .to_string();

        registry.insert(
            id.to_string(),
            ServiceDefinition {
                metadata: raw.metadata,
                compose_template,
                defaults: raw.defaults.unwrap_or_default(),
                health: raw.health,
                storage: raw.storage.map(|s| s.volumes).unwrap_or_default(),
                install_variables: raw.install_variables.unwrap_or_default(),
            },
        );
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_registry_returns_known_services() {
        let registry = load_registry();
        assert!(!registry.is_empty());
        assert!(registry.contains_key("whoami"));
        assert!(registry.contains_key("filebrowser"));
        assert!(registry.contains_key("immich"));
        assert!(registry.contains_key("navidrome"));
    }

    #[test]
    fn whoami_has_correct_metadata() {
        let registry = load_registry();
        let whoami = &registry["whoami"];
        assert_eq!(whoami.metadata.name, "Whoami");
        assert_eq!(whoami.metadata.category, "utilities");
        assert!(!whoami.compose_template.is_empty());
        assert_eq!(whoami.defaults.get("WHOAMI_PORT").unwrap(), "8081");
    }

    #[test]
    fn immich_has_multiple_defaults() {
        let registry = load_registry();
        let immich = &registry["immich"];
        assert!(immich.defaults.contains_key("IMMICH_PORT"));
        assert!(immich.defaults.contains_key("IMMICH_DB_PASSWORD"));
        assert!(immich.compose_template.contains("immich-server"));
    }

    #[test]
    fn all_services_have_health_config() {
        let registry = load_registry();
        for (id, def) in &registry {
            assert!(
                def.health.is_some(),
                "Service {id} missing health config"
            );
        }
    }

    #[test]
    fn filebrowser_has_install_variables() {
        let registry = load_registry();
        let fb = &registry["filebrowser"];
        assert!(fb.storage.is_empty());
        assert_eq!(fb.install_variables.len(), 3);
        let keys: Vec<&str> = fb.install_variables.iter().map(|v| v.key.as_str()).collect();
        assert!(keys.contains(&"BROWSE_PATH"));
        assert!(keys.contains(&"FB_USERNAME"));
        assert!(keys.contains(&"FB_PASSWORD"));
    }

    #[test]
    fn immich_has_storage_volumes() {
        let registry = load_registry();
        let immich = &registry["immich"];
        assert_eq!(immich.storage.len(), 3);
        let names: Vec<&str> = immich.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"upload"));
        assert!(names.contains(&"ml_cache"));
        assert!(names.contains(&"db_data"));
    }

    #[test]
    fn whoami_has_no_storage() {
        let registry = load_registry();
        let whoami = &registry["whoami"];
        assert!(whoami.storage.is_empty());
    }

    #[test]
    fn immich_db_data_has_db_dump_config() {
        let registry = load_registry();
        let immich = &registry["immich"];
        let db_vol = immich.storage.iter().find(|v| v.name == "db_data").unwrap();
        let dump = db_vol.db_dump.as_ref().unwrap();
        assert_eq!(dump.container, "myground-immich-db");
        assert_eq!(dump.command, "pg_dumpall -U postgres");
        assert_eq!(dump.dump_file, "immich_db_dump.sql");
    }

    #[test]
    fn services_without_db_dump_parse_fine() {
        let registry = load_registry();
        let whoami = &registry["whoami"];
        assert!(whoami.storage.is_empty());
    }
}
