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
    /// Command to run inside gluetun when a VPN-forwarded port is assigned.
    /// `{{PORTS}}` is replaced with the actual port number by gluetun.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vpn_port_forward_command: Option<String>,
    /// Shell commands to run inside the main container after Tailscale hostname changes.
    /// `${TAILSCALE_DOMAIN}` and `${SERVER_IP}` are replaced with actual values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_tailscale_change: Vec<String>,
    /// Whether this app supports adding extra read-only folder binds.
    #[serde(default)]
    pub extra_folders: bool,
    /// Link targets this app can connect to (e.g. Sonarr can link to qBittorrent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_targets: Vec<LinkTarget>,
    /// When true, configure authentication via the *arr API after first start.
    /// Requires ARR_USERNAME, ARR_PASSWORD, and ARR_PORT env vars in the compose template.
    #[serde(default)]
    pub arr_config: bool,
}

/// Describes a class of outbound link an app can make to other installed apps.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LinkTarget {
    /// Link type identifier matching `LinkType` enum values (snake_case).
    /// e.g. "download_client", "indexer", "media_server".
    pub link_type: String,
    /// App definition IDs that can fill this role (e.g. `["qbittorrent"]`).
    pub target_app_ids: Vec<String>,
    /// Human-readable label shown in the UI (e.g. "Download Client").
    pub label: String,
    /// Whether this link requires a shared Docker network (`myground-media`).
    /// False for path-based integrations like Jellyfin media server.
    pub required_network: bool,
}

fn default_tailscale_mode() -> String {
    "sidecar".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthConfig {
    /// The port the container listens on internally (used for Tailscale proxy targets).
    /// Optional for apps like Beszel where the container listens on the dynamic host port.
    #[serde(default)]
    pub container_port: Option<u16>,
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
    /// Human-readable hint shown below the input field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Auto-populate the default from another installed app's env var.
    /// Format: `"app_id:ENV_VAR"` (e.g. `"qbittorrent:DOWNLOADS_PATH"`).
    /// If the referenced app is installed, its value replaces `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_from: Option<String>,
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
        assert!(whoami.defaults.is_empty());
    }

    #[test]
    fn immich_has_defaults() {
        let registry = load_registry();
        let immich = &registry["immich"];
        assert!(!immich.defaults.contains_key("IMMICH_PORT"));
        assert!(immich.defaults.contains_key("IMMICH_DB_PASSWORD"));
        assert!(immich.compose_template.contains("immich-server"));
    }

    #[test]
    fn health_config_is_optional() {
        let registry = load_registry();
        // whoami is a scratch image with no shell tools — health config is intentionally absent
        assert!(registry["whoami"].health.is_none());
        // Most apps should have health config
        let with_health = registry.values().filter(|d| d.health.is_some()).count();
        assert!(with_health > 10, "Expected most apps to have health config");
    }

    #[test]
    fn filebrowser_has_install_variables() {
        let registry = load_registry();
        let fb = &registry["filebrowser"];
        assert_eq!(fb.storage.len(), 1);
        assert_eq!(fb.storage[0].name, "browse");
        assert_eq!(fb.install_variables.len(), 2);
        let keys: Vec<&str> = fb
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(keys.contains(&"FB_USERNAME"));
        assert!(keys.contains(&"FB_PASSWORD"));
    }

    #[test]
    fn immich_has_storage_volumes() {
        let registry = load_registry();
        let immich = &registry["immich"];
        assert_eq!(immich.storage.len(), 3);
        let names: Vec<&str> = immich.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"library"));
        assert!(names.contains(&"ml_cache"));
        assert!(names.contains(&"postgres"));
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
        let db_vol = immich
            .storage
            .iter()
            .find(|v| v.name == "postgres")
            .unwrap();
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
        assert_eq!(pihole.metadata.tailscale_mode, "sidecar");
        assert!(pihole.metadata.post_install_notes.is_some());
        assert!(pihole.compose_template.contains("53:53"));
        // AAAA-filter wiring: the template must reference the var that
        // build_merged_env fills with `filter-AAAA` on IPv6-less hosts.
        assert!(pihole
            .compose_template
            .contains("FTLCONF_misc_dnsmasq_lines: \"${PIHOLE_DNSMASQ_LINES}\""));
        assert_eq!(pihole.storage.len(), 2);
        let names: Vec<&str> = pihole.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"pihole_config"));
        assert!(names.contains(&"dnsmasq_config"));
    }

    #[test]
    fn pihole_has_install_variables() {
        let registry = load_registry();
        let pihole = &registry["pihole"];
        assert!(pihole.defaults.is_empty());
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
        assert!(jellyfin.defaults.is_empty());
        assert_eq!(jellyfin.health.as_ref().unwrap().path, "/health");
        assert_eq!(jellyfin.storage.len(), 1);
        assert_eq!(jellyfin.storage[0].name, "config");
        assert_eq!(jellyfin.install_variables.len(), 1);
        assert_eq!(jellyfin.install_variables[0].key, "MEDIA_PATH");
        assert!(jellyfin
            .compose_template
            .contains("jellyfin/jellyfin:latest"));
    }

    #[test]
    fn tdarr_has_safe_managed_media_stack_definition() {
        let registry = load_registry();
        let tdarr = &registry["tdarr"];

        assert_eq!(tdarr.metadata.name, "Tdarr");
        assert_eq!(tdarr.metadata.category, "media");
        assert!(get_app_icon("tdarr").is_some());
        assert!(tdarr.metadata.link_targets.is_empty());
        assert_eq!(
            tdarr.health.as_ref().unwrap().container_port,
            Some(8265)
        );
        assert_eq!(tdarr.health.as_ref().unwrap().path, "/");

        assert_eq!(tdarr.install_variables.len(), 1);
        let media_path = &tdarr.install_variables[0];
        assert_eq!(media_path.key, "MEDIA_PATH");
        assert_eq!(media_path.input_type, "path");
        assert!(media_path.required);
        assert_eq!(media_path.default_from.as_deref(), Some("jellyfin:MEDIA_PATH"));

        let storage: HashMap<&str, &str> = tdarr
            .storage
            .iter()
            .map(|volume| (volume.name.as_str(), volume.container_path.as_str()))
            .collect();
        assert_eq!(storage.len(), 4);
        assert_eq!(storage["server"], "/app/server");
        assert_eq!(storage["configs"], "/app/configs");
        assert_eq!(storage["logs"], "/app/logs");
        assert_eq!(storage["transcode"], "/temp");

        let compose = &tdarr.compose_template;
        assert!(compose.contains("ghcr.io/haveagitgat/tdarr:latest"));
        assert!(compose.contains("${BIND_IP}:${EXIT_PORT}:8265"));
        assert!(!compose.contains(":${EXIT_PORT}:8266"));
        assert!(!compose.contains("8266:8266"));
        assert!(compose.contains("internalNode: \"true\""));
        assert!(compose.contains("nodeName: \"MyGroundInternalNode\""));
        assert!(compose.contains("${MEDIA_PATH}:/media:rw"));
        assert!(compose.contains("PUID: \"${PUID}\""));
        assert!(compose.contains("PGID: \"${PGID}\""));
        assert!(compose.contains("TZ: Etc/UTC"));
        assert!(!compose.contains("ghcr.io/haveagitgat/tdarr_node"));

        let notes = tdarr.metadata.post_install_notes.as_ref().unwrap();
        assert!(notes.contains("/media"));
        assert!(notes.contains("non-destructive workflow"));
        assert!(notes.contains("PUID/PGID"));
        assert!(notes.contains("does not add a library"));
    }

    #[test]
    fn tdarr_install_setup_generates_valid_managed_compose() {
        let registry = load_registry();
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let mut variables = HashMap::new();
        variables.insert(
            "MEDIA_PATH".to_string(),
            "/mnt/myground-test-media".to_string(),
        );

        let result = crate::apps::install_app_setup(
            base,
            &registry,
            "tdarr",
            None,
            Some(&variables),
            None,
            None,
        )
        .unwrap();

        assert_eq!(result.instance_id, "tdarr");
        assert_eq!(result.port, crate::apps::PORT_RANGE_START);

        let compose_path = base.join("apps").join("tdarr").join("docker-compose.yml");
        let compose = std::fs::read_to_string(compose_path).unwrap();
        crate::compose::validate_compose(&compose).unwrap();
        assert!(compose.contains(&format!(
            "127.0.0.1:{}:8265",
            crate::apps::PORT_RANGE_START
        )));
        assert!(!compose.contains("8266:8266"));
        assert!(compose.contains("/mnt/myground-test-media:/media:rw"));
        assert!(compose.contains("internalNode: \"true\""));
        assert!(compose.contains("nodeName: \"MyGroundInternalNode\""));
        assert!(compose.contains("/app/server"));
        assert!(compose.contains("/app/configs"));
        assert!(compose.contains("/app/logs"));
        assert!(compose.contains(":/temp"));

        let state = crate::config::load_app_state(base, "tdarr").unwrap();
        assert!(state.app_links.is_empty());
        assert_eq!(
            state.env_overrides.get("MEDIA_PATH").map(String::as_str),
            Some("/mnt/myground-test-media")
        );
    }

    #[test]
    fn tdarr_icon_is_the_official_upstream_logo() {
        let icon = get_app_icon("tdarr").expect("missing Tdarr SVG icon");
        let svg = std::str::from_utf8(&icon).expect("Tdarr SVG should be UTF-8");

        assert!(svg.contains("Tdarr logo vector"));
        assert!(svg.contains(r#"viewBox="0 0 578 579""#));
        assert!(svg.contains(r##"stroke="#00fff9""##));
        assert!(svg.contains(r##"fill="#00fff9""##));
    }

    #[test]
    fn nextcloud_has_multi_container_setup() {
        let registry = load_registry();
        let nc = &registry["nextcloud"];
        assert_eq!(nc.metadata.name, "Nextcloud");
        assert_eq!(nc.metadata.category, "productivity");
        assert!(!nc.defaults.contains_key("NEXTCLOUD_PORT"));
        assert!(nc.defaults.contains_key("NEXTCLOUD_DB_PASSWORD"));
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
    fn kaneo_has_multi_container_setup() {
        let registry = load_registry();
        let kaneo = &registry["kaneo"];
        assert_eq!(kaneo.metadata.name, "Kaneo");
        assert_eq!(kaneo.metadata.category, "development");
        assert_eq!(kaneo.health.as_ref().unwrap().path, "/api/health");
        assert_eq!(kaneo.storage.len(), 1);
        assert_eq!(kaneo.storage[0].name, "postgres");
        assert_eq!(kaneo.install_variables.len(), 1);
        assert_eq!(kaneo.install_variables[0].key, "KANEO_AUTH_SECRET");
        assert!(kaneo
            .compose_template
            .contains("ghcr.io/usekaneo/api:latest"));
        assert!(kaneo
            .compose_template
            .contains("ghcr.io/usekaneo/web:latest"));
        assert!(kaneo.compose_template.contains("BETTER_AUTH_SECRET"));
        assert!(kaneo.compose_template.contains("${APP_PUBLIC_URL}"));
    }

    #[test]
    fn masterselects_has_correct_metadata_and_compose() {
        let registry = load_registry();
        let masterselects = &registry["masterselects"];
        assert_eq!(masterselects.metadata.name, "MasterSelects");
        assert_eq!(masterselects.metadata.category, "media");
        assert!(!masterselects.metadata.backup_supported);
        assert_eq!(masterselects.metadata.tailscale_mode, "network");
        assert_eq!(
            masterselects.health.as_ref().unwrap().container_port,
            Some(4173)
        );
        assert!(masterselects
            .compose_template
            .contains("Sportinger/MasterSelects"));
        assert!(masterselects
            .compose_template
            .contains("git -C /opt/masterselects fetch"));
        assert!(masterselects
            .compose_template
            .contains("git -C /opt/masterselects reset"));
        assert!(masterselects
            .metadata
            .post_install_notes
            .as_ref()
            .unwrap()
            .contains("window.aiTools"));
        assert!(masterselects.storage.is_empty());
        assert!(masterselects.install_variables.is_empty());
    }

    #[test]
    fn masterselects_icon_uses_catalog_line_style() {
        let icon = get_app_icon("masterselects").expect("missing MasterSelects SVG icon");
        let svg = std::str::from_utf8(&icon).expect("MasterSelects SVG should be UTF-8");

        assert!(svg.contains(r#"width="24""#));
        assert!(svg.contains(r#"height="24""#));
        assert!(svg.contains(r#"viewBox="0 0 24 24""#));
        assert!(svg.contains(r#"fill="none""#));
        assert!(svg.contains(r##"stroke="#a08068""##));
        assert!(!svg.contains(r#"viewBox="0 0 128 128""#));
    }

    #[test]
    fn karakeep_has_correct_metadata_and_compose() {
        let registry = load_registry();
        let karakeep = &registry["karakeep"];
        assert_eq!(karakeep.metadata.name, "Karakeep");
        assert_eq!(karakeep.metadata.category, "productivity");
        assert_eq!(karakeep.metadata.tailscale_mode, "network");
        assert_eq!(karakeep.health.as_ref().unwrap().path, "/api/health");
        assert_eq!(karakeep.health.as_ref().unwrap().container_port, Some(3000));

        let names: Vec<&str> = karakeep.storage.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(karakeep.storage.len(), 2);
        assert!(names.contains(&"data"));
        assert!(names.contains(&"meilisearch"));

        let keys: Vec<&str> = karakeep
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(keys.contains(&"NEXTAUTH_SECRET"));
        assert!(keys.contains(&"MEILI_MASTER_KEY"));

        assert_eq!(karakeep.defaults["KARAKEEP_VERSION"], "release");
        assert!(karakeep
            .compose_template
            .contains("ghcr.io/karakeep-app/karakeep"));
        assert!(karakeep
            .compose_template
            .contains("gcr.io/zenika-hub/alpine-chrome:124"));
        assert!(karakeep
            .compose_template
            .contains("getmeili/meilisearch:v1.41.0"));
        assert!(karakeep.compose_template.contains("${APP_PUBLIC_URL}"));
        assert!(karakeep
            .metadata
            .post_install_notes
            .as_ref()
            .unwrap()
            .contains("NEXTAUTH_URL"));
    }

    #[test]
    fn karakeep_icon_uses_catalog_line_style() {
        let icon = get_app_icon("karakeep").expect("missing Karakeep SVG icon");
        let svg = std::str::from_utf8(&icon).expect("Karakeep SVG should be UTF-8");

        assert!(svg.contains(r#"width="24""#));
        assert!(svg.contains(r#"height="24""#));
        assert!(svg.contains(r#"viewBox="0 0 24 24""#));
        assert!(svg.contains(r#"fill="none""#));
        assert!(svg.contains(r##"stroke="#a08068""##));
        assert!(!svg.contains(r#"viewBox="0 0 128 128""#));
    }

    #[test]
    fn penpot_has_multi_container_setup() {
        let registry = load_registry();
        let penpot = &registry["penpot"];
        assert_eq!(penpot.metadata.name, "Penpot");
        assert_eq!(penpot.metadata.category, "design");
        assert_eq!(penpot.metadata.tailscale_mode, "network");
        assert_eq!(penpot.health.as_ref().unwrap().container_port, Some(8080));
        assert_eq!(penpot.storage.len(), 2);
        let names: Vec<&str> = penpot.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"assets"));
        assert!(names.contains(&"postgres"));
        assert!(penpot.defaults.contains_key("PENPOT_DB_PASSWORD"));
        assert!(penpot
            .install_variables
            .iter()
            .any(|v| v.key == "PENPOT_SECRET_KEY"));
        assert!(penpot
            .install_variables
            .iter()
            .any(|v| v.key == "PENPOT_FLAGS"));
        assert!(penpot
            .compose_template
            .contains("penpotapp/frontend:latest"));
        assert!(penpot.compose_template.contains("penpotapp/backend:latest"));
        assert!(penpot
            .compose_template
            .contains("penpotapp/exporter:latest"));
        assert!(penpot.compose_template.contains("${APP_PUBLIC_URL}"));
    }

    #[test]
    fn paperless_ngx_has_multi_container_setup() {
        let registry = load_registry();
        let paperless = &registry["paperless-ngx"];
        assert_eq!(paperless.metadata.name, "Paperless-ngx");
        assert_eq!(paperless.metadata.category, "files");
        assert_eq!(paperless.metadata.tailscale_mode, "network");
        assert_eq!(
            paperless.health.as_ref().unwrap().container_port,
            Some(8000)
        );
        assert!(paperless
            .metadata
            .post_install_notes
            .as_ref()
            .unwrap()
            .contains("consume storage volume"));
        assert!(paperless
            .install_variables
            .iter()
            .any(|v| v.key == "PAPERLESS_SECRET_KEY"));
        assert!(paperless
            .install_variables
            .iter()
            .any(|v| v.key == "PAPERLESS_DB_PASSWORD"));
        assert!(paperless
            .install_variables
            .iter()
            .any(|v| v.key == "PAPERLESS_OCR_LANGUAGE"));
        assert_eq!(paperless.storage.len(), 6);
        let names: Vec<&str> = paperless.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"data"));
        assert!(names.contains(&"media"));
        assert!(names.contains(&"consume"));
        assert!(names.contains(&"export"));
        assert!(names.contains(&"postgres"));
        assert!(names.contains(&"redis"));
        let db_vol = paperless
            .storage
            .iter()
            .find(|v| v.name == "postgres")
            .unwrap();
        let dump = db_vol.db_dump.as_ref().unwrap();
        assert_eq!(dump.container, "myground-paperless-ngx-db");
        assert_eq!(dump.dump_file, "paperless_ngx_db_dump.sql");
        assert!(paperless
            .compose_template
            .contains("ghcr.io/paperless-ngx/paperless-ngx:latest"));
        assert!(paperless.compose_template.contains("postgres:18-alpine"));
        assert!(paperless.compose_template.contains("redis:8-alpine"));
        assert!(paperless.compose_template.contains("${APP_PUBLIC_URL}"));
    }

    #[test]
    fn paperless_ngx_install_setup_writes_compose() {
        let registry = load_registry();
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let storage = base.join("paperless-storage");
        let mut variables = HashMap::new();
        variables.insert(
            "PAPERLESS_SECRET_KEY".to_string(),
            "test-secret-key-for-paperless".to_string(),
        );
        variables.insert(
            "PAPERLESS_DB_PASSWORD".to_string(),
            "test-db-password-for-paperless".to_string(),
        );
        variables.insert("PAPERLESS_TIME_ZONE".to_string(), "UTC".to_string());
        variables.insert("PAPERLESS_OCR_LANGUAGE".to_string(), "eng".to_string());

        let result = crate::apps::install_app_setup(
            base,
            &registry,
            "paperless-ngx",
            Some(storage.to_str().unwrap()),
            Some(&variables),
            None,
            None,
        )
        .unwrap();

        assert_eq!(result.instance_id, "paperless-ngx");
        assert_eq!(result.port, crate::apps::PORT_RANGE_START);

        let compose = std::fs::read_to_string(
            base.join("apps")
                .join("paperless-ngx")
                .join("docker-compose.yml"),
        )
        .unwrap();
        assert!(compose.contains("myground-paperless-ngx"));
        assert!(compose.contains("paperless-ngx-db"));
        assert!(compose.contains("paperless-ngx-redis"));
        assert!(compose.contains("PAPERLESS_DBPASS: \"test-db-password-for-paperless\""));
        assert!(compose.contains("/usr/src/paperless/consume"));

        let env =
            std::fs::read_to_string(base.join("apps").join("paperless-ngx").join(".env")).unwrap();
        assert!(env.contains("PAPERLESS_SECRET_KEY=test-secret-key-for-paperless"));
        assert!(env.contains("STORAGE_consume="));
        assert!(storage.join("consume").exists());
        assert!(storage.join("media").exists());
        assert!(storage.join("postgres").exists());
    }

    #[test]
    fn anytype_has_self_hosted_sync_stack() {
        let registry = load_registry();
        let anytype = &registry["anytype"];
        assert_eq!(anytype.metadata.name, "Anytype");
        assert_eq!(anytype.metadata.category, "productivity");
        assert_eq!(anytype.metadata.tailscale_mode, "skip");
        assert!(!anytype.metadata.backup_supported);
        assert!(anytype.health.is_none());
        assert_eq!(anytype.storage.len(), 2);
        let names: Vec<&str> = anytype.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"config"));
        assert!(names.contains(&"data"));
        let keys: Vec<&str> = anytype
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(keys.contains(&"EXTERNAL_LISTEN_HOSTS"));
        assert!(keys.contains(&"ANYTYPE_BIND_IP"));
        assert_eq!(
            anytype
                .defaults
                .get("ANY_SYNC_DOCKERCOMPOSE_TAG")
                .map(String::as_str),
            Some("v7.0.0")
        );
        assert!(anytype
            .compose_template
            .contains("ghcr.io/anyproto/any-sync-node"));
        assert!(anytype
            .compose_template
            .contains("Dockerfile-any-sync-init"));
        assert!(anytype
            .metadata
            .post_install_notes
            .as_ref()
            .unwrap()
            .contains("client.yml"));
    }

    #[test]
    fn anytype_icon_uses_catalog_line_style() {
        let icon = get_app_icon("anytype").expect("missing Anytype SVG icon");
        let svg = std::str::from_utf8(&icon).expect("Anytype SVG should be UTF-8");

        assert!(svg.contains(r#"width="24""#));
        assert!(svg.contains(r#"height="24""#));
        assert!(svg.contains(r#"viewBox="0 0 24 24""#));
        assert!(svg.contains(r#"fill="none""#));
        assert!(svg.contains(r##"stroke="#a08068""##));
    }

    #[test]
    fn vaultwarden_has_correct_metadata_and_storage() {
        let registry = load_registry();
        let vw = &registry["vaultwarden"];
        assert_eq!(vw.metadata.name, "Vaultwarden");
        assert_eq!(vw.metadata.category, "security");
        assert!(vw.defaults.is_empty());
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
        assert!(qbt.defaults.is_empty());
        assert_eq!(qbt.health.as_ref().unwrap().path, "/api/v2/app/version");
        assert_eq!(qbt.storage.len(), 1);
        assert_eq!(qbt.storage[0].name, "config");
        assert_eq!(qbt.install_variables.len(), 3);
        let keys: Vec<&str> = qbt
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(keys.contains(&"DOWNLOADS_PATH"));
        assert!(keys.contains(&"QB_USERNAME"));
        assert!(keys.contains(&"QB_PASSWORD"));
        assert!(qbt
            .compose_template
            .contains("linuxserver/qbittorrent:latest"));
    }

    #[test]
    fn stirling_pdf_has_official_image_storage_and_security_defaults() {
        let registry = load_registry();
        let stirling = &registry["stirling-pdf"];
        assert_eq!(stirling.metadata.name, "Stirling PDF");
        assert_eq!(stirling.metadata.category, "productivity");
        assert_eq!(stirling.health.as_ref().unwrap().container_port, Some(8080));
        assert_eq!(stirling.health.as_ref().unwrap().path, "/");
        assert_eq!(stirling.storage.len(), 5);

        let names: Vec<&str> = stirling.storage.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"configs"));
        assert!(names.contains(&"custom_files"));
        assert!(names.contains(&"logs"));
        assert!(names.contains(&"pipeline"));
        assert!(names.contains(&"tessdata"));

        let variable_keys: Vec<&str> = stirling
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(variable_keys.contains(&"STIRLING_ADMIN_USERNAME"));
        assert!(variable_keys.contains(&"STIRLING_ADMIN_PASSWORD"));
        assert!(variable_keys.contains(&"STIRLING_ENABLE_LOGIN"));
        assert!(variable_keys.contains(&"STIRLING_LANGS"));
        assert!(variable_keys.contains(&"STIRLING_DEFAULT_LOCALE"));

        assert!(stirling
            .compose_template
            .contains("stirlingtools/stirling-pdf:latest"));
        assert!(stirling
            .compose_template
            .contains("DISABLE_ADDITIONAL_FEATURES"));
        assert!(stirling.compose_template.contains("SECURITY_ENABLELOGIN"));
        assert!(stirling
            .compose_template
            .contains("SECURITY_INITIALLOGIN_PASSWORD"));
        assert!(stirling
            .compose_template
            .contains("${STORAGE_custom_files}:/customFiles:rw"));
        assert!(stirling
            .metadata
            .post_install_notes
            .as_ref()
            .unwrap()
            .contains("authentication enabled"));
    }

    #[test]
    fn voicebox_has_source_build_storage_and_security_notes() {
        let registry = load_registry();
        let voicebox = &registry["voicebox"];

        assert_eq!(voicebox.metadata.name, "Voicebox");
        assert_eq!(voicebox.metadata.category, "ai");
        assert_eq!(
            voicebox.health.as_ref().unwrap().container_port,
            Some(17493)
        );
        assert_eq!(voicebox.health.as_ref().unwrap().path, "/health");
        assert_eq!(voicebox.metadata.gpu_apps, vec!["voicebox".to_string()]);

        let names: Vec<&str> = voicebox.storage.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(voicebox.storage.len(), 3);
        assert!(names.contains(&"generations"));
        assert!(names.contains(&"data"));
        assert!(names.contains(&"model_cache"));

        let variable_keys: Vec<&str> = voicebox
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(variable_keys.contains(&"LOG_LEVEL"));
        assert!(variable_keys.contains(&"VOICEBOX_MODELS_DIR"));
        assert!(variable_keys.contains(&"VOICEBOX_CORS_ORIGINS"));
        assert!(variable_keys.contains(&"VOICEBOX_CPU_LIMIT"));
        assert!(variable_keys.contains(&"VOICEBOX_MEMORY_LIMIT"));

        assert_eq!(voicebox.defaults["VOICEBOX_REF"], "main");
        assert!(voicebox
            .compose_template
            .contains("https://github.com/jamiepine/voicebox.git#${VOICEBOX_REF}"));
        assert!(voicebox
            .compose_template
            .contains("${STORAGE_generations}:/app/data/generations"));
        assert!(voicebox
            .compose_template
            .contains("${STORAGE_data}:/app/data"));
        assert!(voicebox
            .compose_template
            .contains("${STORAGE_model_cache}:${VOICEBOX_MODELS_DIR}"));
        assert!(voicebox.compose_template.contains("VOICEBOX_CORS_ORIGINS"));
        assert!(voicebox.compose_template.contains("voicebox-init"));
        assert!(voicebox
            .compose_template
            .contains("service_completed_successfully"));
        assert!(voicebox
            .compose_template
            .contains("chown -R voicebox:voicebox"));
        assert!(voicebox
            .compose_template
            .contains("curl -fsS http://127.0.0.1:17493/health"));

        let notes = voicebox.metadata.post_install_notes.as_ref().unwrap();
        assert!(notes.contains("no built-in authentication"));
        assert!(notes.contains("Hugging Face"));
        assert!(notes.contains("NVIDIA"));
        assert!(notes.contains("ROCm"));
        assert!(notes.contains("image-digest update checks cannot detect"));
    }

    #[test]
    fn voicebox_icon_uses_catalog_line_style() {
        let icon = get_app_icon("voicebox").expect("missing Voicebox SVG icon");
        let svg = std::str::from_utf8(&icon).expect("Voicebox SVG should be UTF-8");

        assert!(svg.contains(r#"width="24""#));
        assert!(svg.contains(r#"height="24""#));
        assert!(svg.contains(r#"viewBox="0 0 24 24""#));
        assert!(svg.contains(r#"fill="none""#));
        assert!(svg.contains(r##"stroke="#a08068""##));
        assert!(!svg.contains(r#"viewBox="0 0 128 128""#));
    }

    #[test]
    fn stemdeck_has_source_build_storage_and_security_notes() {
        let registry = load_registry();
        let stemdeck = &registry["stemdeck"];

        assert_eq!(stemdeck.metadata.name, "StemDeck");
        assert_eq!(stemdeck.metadata.category, "media");
        assert_eq!(
            stemdeck.health.as_ref().unwrap().container_port,
            Some(8000)
        );
        assert_eq!(stemdeck.health.as_ref().unwrap().path, "/");
        assert_eq!(stemdeck.metadata.gpu_apps, vec!["stemdeck".to_string()]);

        let names: Vec<&str> = stemdeck.storage.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(stemdeck.storage.len(), 2);
        assert!(names.contains(&"jobs"));
        assert!(names.contains(&"cache"));

        let variable_keys: Vec<&str> = stemdeck
            .install_variables
            .iter()
            .map(|v| v.key.as_str())
            .collect();
        assert!(variable_keys.contains(&"STEMDECK_DEMUCS_DEVICE"));
        assert!(variable_keys.contains(&"STEMDECK_MAX_DURATION_SEC"));

        assert_eq!(stemdeck.defaults["STEMDECK_REF"], "main");
        assert!(stemdeck
            .compose_template
            .contains("https://github.com/stemdeckapp/stemdeck.git#${STEMDECK_REF}"));
        assert!(stemdeck
            .compose_template
            .contains("dockerfile: build/Dockerfile"));
        assert!(stemdeck.compose_template.contains("${STORAGE_jobs}:/app/jobs"));
        assert!(stemdeck.compose_template.contains("${STORAGE_cache}:/cache"));

        let notes = stemdeck.metadata.post_install_notes.as_ref().unwrap();
        assert!(notes.contains("no built-in authentication"));
        assert!(notes.contains("Demucs"));
        assert!(notes.contains("image-digest update checks cannot detect"));
    }
}
