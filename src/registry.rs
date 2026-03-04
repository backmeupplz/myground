use std::collections::HashMap;

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(RustEmbed)]
#[folder = "apps"]
struct AppFiles;

#[derive(Debug, Clone, Deserialize)]
struct RawAppDefinition {
    metadata: AppMetadata,
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
pub struct AppMetadata {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub website: String,
    pub category: String,
    #[serde(default = "default_true")]
    pub backup_supported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_install_notes: Option<String>,
    /// Extra path appended to the app URL when opening (e.g. "/admin").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_path: Option<String>,
    /// Tailscale sidecar mode: "sidecar" (default), "network", or "skip".
    #[serde(default = "default_tailscale_mode")]
    pub tailscale_mode: String,
    /// Compose keys that should receive GPU injection. Empty = GPU not supported.
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "gpu_services")]
    pub gpu_apps: Vec<String>,
}

fn default_tailscale_mode() -> String {
    "sidecar".to_string()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_command: Option<String>,
    /// Command to wipe/drop the database before restoring (runs inside the container).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wipe_command: Option<String>,
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
pub struct AppDefinition {
    pub metadata: AppMetadata,
    pub compose_template: String,
    pub defaults: HashMap<String, String>,
    pub health: Option<HealthConfig>,
    pub storage: Vec<StorageVolume>,
    pub install_variables: Vec<InstallVariable>,
}

/// Load all embedded app definitions by auto-discovering TOML files in `apps/`.
pub fn load_registry() -> HashMap<String, AppDefinition> {
    let mut registry = HashMap::new();

    for filename in AppFiles::iter() {
        let filename_str = filename.as_ref();
        if !filename_str.ends_with(".toml") {
            continue;
        }
        let id = filename_str.trim_end_matches(".toml");
        let data = AppFiles::get(filename_str)
            .unwrap_or_else(|| panic!("Failed to read embedded file {filename_str}"));
        let toml_str = std::str::from_utf8(data.data.as_ref())
            .unwrap_or_else(|e| panic!("Invalid UTF-8 in {filename_str}: {e}"));

        let mut raw: RawAppDefinition = toml::from_str(toml_str)
            .unwrap_or_else(|e| panic!("Failed to parse {filename_str}: {e}"));
        raw.metadata.id = id.to_string();

        let yml_filename = format!("{id}.yml");
        let yml_data = AppFiles::get(&yml_filename)
            .unwrap_or_else(|| panic!("Missing compose file {yml_filename}"));
        let compose_template = std::str::from_utf8(yml_data.data.as_ref())
            .unwrap_or_else(|e| panic!("Invalid UTF-8 in {yml_filename}: {e}"))
            .to_string();

        registry.insert(
            id.to_string(),
            AppDefinition {
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

/// Get the embedded SVG icon for an app, if it exists.
pub fn get_app_icon(id: &str) -> Option<Vec<u8>> {
    let filename = format!("{id}.svg");
    AppFiles::get(&filename).map(|f| f.data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_registry_returns_all_embedded_apps() {
        let registry = load_registry();
        let expected: Vec<String> = AppFiles::iter()
            .filter(|f| f.as_ref().ends_with(".toml"))
            .map(|f| f.as_ref().trim_end_matches(".toml").to_string())
            .collect();
        assert!(!expected.is_empty());
        for id in &expected {
            assert!(
                registry.contains_key(id.as_str()),
                "App {id} has a .toml but is missing from registry"
            );
        }
        assert_eq!(registry.len(), expected.len());
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
    fn all_apps_have_health_config() {
        let registry = load_registry();
        for (id, def) in &registry {
            assert!(
                def.health.is_some(),
                "App {id} missing health config"
            );
        }
    }

    #[test]
    fn filebrowser_has_install_variables() {
        let registry = load_registry();
        let fb = &registry["filebrowser"];
        assert_eq!(fb.storage.len(), 1);
        assert_eq!(fb.storage[0].name, "browse");
        assert_eq!(fb.install_variables.len(), 2);
        let keys: Vec<&str> = fb.install_variables.iter().map(|v| v.key.as_str()).collect();
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
    fn apps_without_db_dump_parse_fine() {
        let registry = load_registry();
        let whoami = &registry["whoami"];
        assert!(whoami.storage.is_empty());
    }

    #[test]
    fn pihole_has_correct_metadata_and_storage() {
        let registry = load_registry();
        let pihole = &registry["pihole"];
        assert_eq!(pihole.metadata.name, "Pi-hole");
        assert_eq!(pihole.metadata.category, "network");
        assert!(pihole.metadata.post_install_notes.is_some());
        assert!(pihole.compose_template.contains("53:53"));
        assert_eq!(pihole.storage.len(), 2);
        let names: Vec<&str> = pihole.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"pihole_config"));
        assert!(names.contains(&"dnsmasq_config"));
    }

    #[test]
    fn pihole_has_port_default_and_install_variables() {
        let registry = load_registry();
        let pihole = &registry["pihole"];
        assert_eq!(pihole.defaults.get("PIHOLE_PORT").unwrap(), "8086");
        assert_eq!(pihole.install_variables.len(), 1);
        assert_eq!(pihole.install_variables[0].key, "PIHOLE_PASSWORD");
        assert_eq!(pihole.install_variables[0].input_type, "password");
    }

    #[test]
    fn whoami_has_no_post_install_notes() {
        let registry = load_registry();
        let whoami = &registry["whoami"];
        assert!(whoami.metadata.post_install_notes.is_none());
    }

    #[test]
    fn post_install_notes_contains_placeholders() {
        let registry = load_registry();
        let pihole = &registry["pihole"];
        let notes = pihole.metadata.post_install_notes.as_ref().unwrap();
        assert!(notes.contains("${SERVER_IP}"));
        assert!(notes.contains("${PORT}"));
    }

    #[test]
    fn jellyfin_has_correct_metadata_and_storage() {
        let registry = load_registry();
        let jellyfin = &registry["jellyfin"];
        assert_eq!(jellyfin.metadata.name, "Jellyfin");
        assert_eq!(jellyfin.metadata.category, "media");
        assert_eq!(jellyfin.defaults.get("JELLYFIN_PORT").unwrap(), "8087");
        assert_eq!(jellyfin.health.as_ref().unwrap().port, 8087);
        assert_eq!(jellyfin.health.as_ref().unwrap().path, "/health");
        assert_eq!(jellyfin.storage.len(), 1);
        assert_eq!(jellyfin.storage[0].name, "config");
        assert_eq!(jellyfin.install_variables.len(), 1);
        assert_eq!(jellyfin.install_variables[0].key, "MEDIA_PATH");
        assert!(jellyfin.compose_template.contains("jellyfin/jellyfin:latest"));
    }

    #[test]
    fn nextcloud_has_multi_container_setup() {
        let registry = load_registry();
        let nc = &registry["nextcloud"];
        assert_eq!(nc.metadata.name, "Nextcloud");
        assert_eq!(nc.metadata.category, "productivity");
        assert_eq!(nc.defaults.get("NEXTCLOUD_PORT").unwrap(), "8088");
        assert!(nc.defaults.contains_key("NEXTCLOUD_DB_PASSWORD"));
        assert_eq!(nc.health.as_ref().unwrap().port, 8088);
        assert_eq!(nc.health.as_ref().unwrap().path, "/status.php");
        assert_eq!(nc.storage.len(), 2);
        let names: Vec<&str> = nc.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"data"));
        assert!(names.contains(&"db_data"));
        assert_eq!(nc.install_variables.len(), 2);
        assert!(nc.compose_template.contains("nextcloud-db"));
        assert!(nc.compose_template.contains("nextcloud-redis"));
    }

    #[test]
    fn nextcloud_db_data_has_db_dump_config() {
        let registry = load_registry();
        let nc = &registry["nextcloud"];
        let db_vol = nc.storage.iter().find(|v| v.name == "db_data").unwrap();
        let dump = db_vol.db_dump.as_ref().unwrap();
        assert_eq!(dump.container, "myground-nextcloud-db");
        assert_eq!(dump.dump_file, "nextcloud_db_dump.sql");
    }

    #[test]
    fn vaultwarden_has_correct_metadata_and_storage() {
        let registry = load_registry();
        let vw = &registry["vaultwarden"];
        assert_eq!(vw.metadata.name, "Vaultwarden");
        assert_eq!(vw.metadata.category, "security");
        assert_eq!(vw.defaults.get("VAULTWARDEN_PORT").unwrap(), "8089");
        assert_eq!(vw.health.as_ref().unwrap().port, 8089);
        assert_eq!(vw.health.as_ref().unwrap().path, "/alive");
        assert_eq!(vw.storage.len(), 1);
        assert_eq!(vw.storage[0].name, "data");
        assert_eq!(vw.install_variables.len(), 1);
        assert_eq!(vw.install_variables[0].key, "ADMIN_TOKEN");
        assert_eq!(vw.install_variables[0].input_type, "password");
        assert!(vw.compose_template.contains("vaultwarden/server:latest"));
    }

    #[test]
    fn qbittorrent_has_correct_metadata_and_storage() {
        let registry = load_registry();
        let qbt = &registry["qbittorrent"];
        assert_eq!(qbt.metadata.name, "qBittorrent");
        assert_eq!(qbt.metadata.category, "downloads");
        assert_eq!(qbt.defaults.get("QBITTORRENT_PORT").unwrap(), "8090");
        assert_eq!(qbt.health.as_ref().unwrap().port, 8090);
        assert_eq!(qbt.health.as_ref().unwrap().path, "/api/v2/app/version");
        assert_eq!(qbt.storage.len(), 1);
        assert_eq!(qbt.storage[0].name, "config");
        assert_eq!(qbt.install_variables.len(), 3);
        let keys: Vec<&str> = qbt.install_variables.iter().map(|v| v.key.as_str()).collect();
        assert!(keys.contains(&"DOWNLOADS_PATH"));
        assert!(keys.contains(&"QB_USERNAME"));
        assert!(keys.contains(&"QB_PASSWORD"));
        assert!(qbt.compose_template.contains("linuxserver/qbittorrent:latest"));
    }
}
