use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize)]
struct RawServiceDefinition {
    metadata: ServiceMetadata,
    compose: ComposeConfig,
    defaults: Option<HashMap<String, String>>,
    health: Option<HealthConfig>,
    storage: Option<StorageConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageConfig {
    volumes: Vec<StorageVolume>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub website: String,
    pub category: String,
    #[serde(default)]
    pub multi_instance: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ComposeConfig {
    template: String,
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
}

/// Load all embedded service definitions from compiled TOML.
pub fn load_registry() -> HashMap<String, ServiceDefinition> {
    let raw_tomls: &[(&str, &str)] = &[
        ("whoami", include_str!("registry/whoami.toml")),
        ("filebrowser", include_str!("registry/filebrowser.toml")),
        ("immich", include_str!("registry/immich.toml")),
    ];

    let mut registry = HashMap::new();

    for (id, toml_str) in raw_tomls {
        let raw: RawServiceDefinition =
            toml::from_str(toml_str).unwrap_or_else(|e| panic!("Failed to parse {id}.toml: {e}"));

        assert_eq!(
            raw.metadata.id, *id,
            "Service ID mismatch in {id}.toml: expected {id}, got {}",
            raw.metadata.id
        );

        registry.insert(
            id.to_string(),
            ServiceDefinition {
                metadata: raw.metadata,
                compose_template: raw.compose.template,
                defaults: raw.defaults.unwrap_or_default(),
                health: raw.health,
                storage: raw.storage.map(|s| s.volumes).unwrap_or_default(),
            },
        );
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_registry_returns_three_services() {
        let registry = load_registry();
        assert_eq!(registry.len(), 3);
        assert!(registry.contains_key("whoami"));
        assert!(registry.contains_key("filebrowser"));
        assert!(registry.contains_key("immich"));
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
    fn filebrowser_has_storage_volumes() {
        let registry = load_registry();
        let fb = &registry["filebrowser"];
        assert_eq!(fb.storage.len(), 2);
        assert!(fb.storage.iter().any(|v| v.name == "data"));
        assert!(fb.storage.iter().any(|v| v.name == "config"));
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
        let fb = &registry["filebrowser"];
        for vol in &fb.storage {
            assert!(vol.db_dump.is_none());
        }
        let whoami = &registry["whoami"];
        assert!(whoami.storage.is_empty());
    }
}
