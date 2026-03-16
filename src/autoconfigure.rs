//! Auto-configuration engine for the *arr stack.
//!
//! Wires up Sonarr, Radarr, Prowlarr, and qBittorrent after containers start.
//! Called when links are added/changed or when an app with links is started.

use std::path::Path;

use serde_json::json;
use tracing::{info, warn};

use crate::config;
use crate::error::AppError;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a reqwest client with a 10-second timeout.
fn http_client(cookies: bool) -> Result<reqwest::Client, AppError> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10));
    if cookies {
        builder = builder.cookie_store(true);
    }
    builder.build().map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))
}

/// Container hostname on the shared Docker network.
///
/// When a container uses `network_mode: service:X`, it shares X's network
/// namespace, so other containers must use X's container name.
///
/// Reads the compose file to check for `network_mode: service:...` and
/// returns the referenced service's container name.
fn container_host(data_dir: &Path, app_id: &str, state: &config::InstalledAppState) -> String {
    let vpn_on = state.vpn.as_ref().is_some_and(|v| v.enabled);
    if vpn_on {
        // VPN active: app uses network_mode: service:gluetun
        return format!("myground-{app_id}-vpn");
    }

    // Check the compose file for network_mode: service:ts-sidecar
    let compose_path = config::app_dir(data_dir, app_id).join("docker-compose.yml");
    if let Ok(yaml) = std::fs::read_to_string(&compose_path) {
        if let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&yaml) {
            if let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) {
                // Find the first (main) service
                if let Some((_, main_svc)) = services.iter().next() {
                    if let Some(nm) = main_svc.get("network_mode").and_then(|v| v.as_str()) {
                        if let Some(ref_svc) = nm.strip_prefix("service:") {
                            // Find the referenced service's container_name
                            if let Some(ref_svc_val) = services.get(ref_svc) {
                                if let Some(cn) = ref_svc_val.get("container_name").and_then(|v| v.as_str()) {
                                    return cn.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    format!("myground-{app_id}")
}

/// Base app type for an instance (e.g. "sonarr" for both "sonarr" and "sonarr-2").
fn def_id<'a>(app_id: &'a str, state: &'a config::InstalledAppState) -> &'a str {
    state.definition_id.as_deref().unwrap_or(app_id)
}

/// Extract text content from a simple XML tag: `<Tag>value</Tag>` → `"value"`.
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

/// Health-check URL path for each app type.
fn health_path_for(app_type: &str) -> &'static str {
    match app_type {
        "sonarr" | "radarr" => "/api/v3/health",
        "prowlarr" => "/api/v1/health",
        "qbittorrent" => "/api/v2/app/version",
        _ => "/",
    }
}

/// Internal Docker port for each app type.
fn internal_port_for(app_type: &str) -> u16 {
    match app_type {
        "sonarr" => 8989,
        "radarr" => 7878,
        "prowlarr" => 9696,
        "qbittorrent" => 8080,
        _ => 8080,
    }
}

/// Find an existing entry by name in an *arr API list response.
///
/// Returns the `id` field if a matching `name` is found.
fn find_existing_by_name(items: &[serde_json::Value], name: &str) -> Option<i64> {
    items
        .iter()
        .find_map(|item| (item["name"].as_str() == Some(name)).then(|| item["id"].as_i64()).flatten())
}

/// Create or update an entry via an *arr API endpoint.
///
/// If `existing_id` is `Some`, sends PUT to `{base_url}/{endpoint}/{id}`.
/// Otherwise sends POST to `{base_url}/{endpoint}`.
async fn upsert(
    client: &reqwest::Client,
    base_url: &str,
    endpoint: &str,
    api_key: &str,
    existing_id: Option<i64>,
    payload: &serde_json::Value,
    label: &str,
) -> Result<(), AppError> {
    let resp = match existing_id {
        Some(id) => {
            client
                .put(format!("{base_url}/{endpoint}/{id}"))
                .header("X-Api-Key", api_key)
                .json(payload)
                .send()
                .await
        }
        None => {
            client
                .post(format!("{base_url}/{endpoint}"))
                .header("X-Api-Key", api_key)
                .json(payload)
                .send()
                .await
        }
    }
    .map_err(|e| AppError::Io(format!("{label} request failed: {e}")))?;

    if resp.status().is_success() {
        info!("{label}: success");
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!("{label} failed ({status}): {body}")))
    }
}

/// List entries from an *arr API endpoint and parse as JSON array.
async fn list_arr_entries(
    client: &reqwest::Client,
    base_url: &str,
    endpoint: &str,
    api_key: &str,
    label: &str,
) -> Result<Vec<serde_json::Value>, AppError> {
    let resp = client
        .get(format!("{base_url}/{endpoint}"))
        .header("X-Api-Key", api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("{label} GET failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Io(format!(
            "{label} returned {}",
            resp.status()
        )));
    }

    resp.json()
        .await
        .map_err(|e| AppError::Io(format!("{label} parse failed: {e}")))
}

// ── Core functions ──────────────────────────────────────────────────────────

/// Poll an app's health endpoint until it returns HTTP 200 or 401.
pub async fn wait_for_app_ready(
    port: u16,
    health_path: &str,
    timeout_secs: u64,
) -> Result<(), AppError> {
    let url = format!("http://localhost:{port}{health_path}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if let Ok(resp) = client.get(&url).send().await {
            let status = resp.status().as_u16();
            if resp.status().is_success() || status == 401 {
                return Ok(());
            }
        }

        if std::time::Instant::now() >= deadline {
            return Err(AppError::Io(format!(
                "App on port {port} did not become ready within {timeout_secs}s"
            )));
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

/// Read the API key for a Sonarr/Radarr/Prowlarr instance from its config.xml.
///
/// Retries up to 10 times with 3s delays to handle the race between container
/// startup and the file appearing on disk.
pub async fn get_arr_api_key(data_dir: &Path, app_id: &str) -> Result<String, AppError> {
    let state = config::load_app_state(data_dir, app_id)?;

    let config_path = state
        .storage_paths
        .get("config")
        .ok_or_else(|| AppError::Io(format!("'{app_id}' has no 'config' storage path")))?;

    let xml_path = std::path::PathBuf::from(config_path).join("config.xml");

    for attempt in 0..10u32 {
        if xml_path.exists() {
            let content = std::fs::read_to_string(&xml_path)
                .map_err(|e| AppError::Io(format!("Read {}: {e}", xml_path.display())))?;

            if let Some(key) = extract_xml_tag(&content, "ApiKey") {
                return Ok(key);
            }
        }

        if attempt < 9 {
            tracing::debug!("config.xml not ready for {app_id} (attempt {}/10)", attempt + 1);
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    }

    Err(AppError::Io(format!(
        "Could not read API key for '{app_id}' after retries"
    )))
}

// ── Arr authentication ──────────────────────────────────────────────────────

/// Configure authentication on an *arr app (Sonarr/Radarr/Prowlarr) via its API.
///
/// Reads ARR_USERNAME and ARR_PASSWORD from the app's env overrides, waits for
/// the app to become healthy, then sets Forms auth via the host config API.
/// Only runs once — skips if a `.arr_auth_done` flag file exists.
pub async fn configure_arr_auth(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, app_id)?;
    let app_type = def_id(app_id, &state);

    let port = state
        .port
        .ok_or_else(|| AppError::Io(format!("No port for '{app_id}'")))?;

    let username = state
        .env_overrides
        .get("ARR_USERNAME")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());
    let password = match state.env_overrides.get("ARR_PASSWORD") {
        Some(p) if !p.is_empty() => p.clone(),
        _ => {
            info!("No ARR_PASSWORD for {app_id}, skipping auth setup");
            return Ok(());
        }
    };

    // Check if auth was already configured
    let config_path = state
        .storage_paths
        .get("config")
        .ok_or_else(|| AppError::Io(format!("'{app_id}' has no 'config' storage path")))?;
    let flag_path = std::path::PathBuf::from(config_path).join(".arr_auth_done");
    if flag_path.exists() {
        info!("Auth already configured for {app_id}, skipping");
        return Ok(());
    }

    // Wait for health
    let health_path = health_path_for(app_type);
    info!("Waiting for {app_id} to become ready before setting auth...");
    wait_for_app_ready(port, health_path, 120).await?;

    // Read API key
    let api_key = get_arr_api_key(data_dir, app_id).await?;

    // Determine API version
    let api_ver = match app_type {
        "prowlarr" => "v1",
        _ => "v3",
    };

    let client = http_client(false)?;
    let base_url = format!("http://localhost:{port}/api/{api_ver}");

    // GET current host config
    let resp = client
        .get(format!("{base_url}/config/host"))
        .header("X-Api-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("GET config/host for {app_id}: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Io(format!(
            "GET config/host for {app_id} returned {}",
            resp.status()
        )));
    }

    let mut host_config: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse config/host for {app_id}: {e}")))?;

    // Set auth fields
    host_config["authenticationMethod"] = json!("forms");
    host_config["authenticationRequired"] = json!("enabled");
    host_config["username"] = json!(username);
    host_config["password"] = json!(password);
    host_config["passwordConfirmation"] = json!(password);

    // PUT updated config
    let resp = client
        .put(format!("{base_url}/config/host"))
        .header("X-Api-Key", &api_key)
        .json(&host_config)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("PUT config/host for {app_id}: {e}")))?;

    if resp.status().is_success() {
        info!("Auth configured for {app_id}: user={username}");
        // Write flag file so we don't reconfigure on restart
        let _ = std::fs::write(&flag_path, "done");
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "PUT config/host for {app_id} failed ({status}): {body}"
        )))
    }
}

// ── Per-app configuration ───────────────────────────────────────────────────

/// Configuration that differs between Sonarr and Radarr.
struct ArrConfig {
    /// Download category name (e.g. "tv-sonarr", "movies-radarr").
    category: &'static str,
    /// Download category field name in the API (e.g. "tvCategory", "movieCategory").
    category_field: &'static str,
    /// Priority field names (e.g. "recentTvPriority" / "recentMoviePriority").
    recent_priority_field: &'static str,
    older_priority_field: &'static str,
    /// Root folder path inside the container (e.g. "/tv", "/movies").
    root_path: &'static str,
}

const SONARR_CONFIG: ArrConfig = ArrConfig {
    category: "tv-sonarr",
    category_field: "tvCategory",
    recent_priority_field: "recentTvPriority",
    older_priority_field: "olderTvPriority",
    root_path: "/tv",
};

const RADARR_CONFIG: ArrConfig = ArrConfig {
    category: "movies-radarr",
    category_field: "movieCategory",
    recent_priority_field: "recentMoviePriority",
    older_priority_field: "olderMoviePriority",
    root_path: "/movies",
};

fn arr_config_for(app_type: &str) -> Option<&'static ArrConfig> {
    match app_type {
        "sonarr" => Some(&SONARR_CONFIG),
        "radarr" => Some(&RADARR_CONFIG),
        _ => None,
    }
}

/// Configure qBittorrent as the download client in a Sonarr or Radarr instance.
async fn configure_arr_download_client(
    data_dir: &Path,
    arr_id: &str,
    qbt_id: &str,
    cfg: &ArrConfig,
) -> Result<(), AppError> {
    let arr_state = config::load_app_state(data_dir, arr_id)?;
    let qbt_state = config::load_app_state(data_dir, qbt_id)?;

    let arr_port = arr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port for '{arr_id}'")))?;
    let api_key = get_arr_api_key(data_dir, arr_id).await?;

    let qbt_host = container_host(data_dir, qbt_id, &qbt_state);
    let qbt_user = qbt_state.env_overrides.get("QB_USERNAME")
        .cloned().unwrap_or_else(|| "admin".into());
    let qbt_pass = qbt_state.env_overrides.get("QB_PASSWORD")
        .cloned().unwrap_or_else(|| "adminadmin".into());

    let client = http_client(false)?;
    let base_url = format!("http://localhost:{arr_port}/api/v3");
    let label = format!("Configure qBittorrent in {arr_id}");

    let clients = list_arr_entries(&client, &base_url, "downloadclient", &api_key, &label).await?;
    let existing_id = find_existing_by_name(&clients, "qBittorrent (MyGround)");

    let payload = json!({
        "name": "qBittorrent (MyGround)",
        "implementation": "QBittorrent",
        "configContract": "QBittorrentSettings",
        "protocol": "torrent",
        "enable": true,
        "priority": 1,
        "tags": [],
        "fields": [
            {"name": "host", "value": qbt_host},
            {"name": "port", "value": internal_port_for("qbittorrent")},
            {"name": "username", "value": qbt_user},
            {"name": "password", "value": qbt_pass},
            {"name": cfg.category_field, "value": cfg.category},
            {"name": cfg.recent_priority_field, "value": 0},
            {"name": cfg.older_priority_field, "value": 0},
            {"name": "initialState", "value": 0},
            {"name": "sequentialOrder", "value": false},
            {"name": "firstAndLast", "value": false}
        ]
    });

    upsert(&client, &base_url, "downloadclient", &api_key, existing_id, &payload, &label).await
}

/// Ensure a root folder exists in a Sonarr or Radarr instance.
async fn configure_arr_root_folder(
    data_dir: &Path,
    arr_id: &str,
    cfg: &ArrConfig,
) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, arr_id)?;
    let port = state.port.ok_or_else(|| AppError::Io(format!("No port for '{arr_id}'")))?;
    let api_key = get_arr_api_key(data_dir, arr_id).await?;

    let client = http_client(false)?;
    let base_url = format!("http://localhost:{port}/api/v3");
    let label = format!("Root folder {path} in {arr_id}", path = cfg.root_path);

    let folders = list_arr_entries(&client, &base_url, "rootfolder", &api_key, &label).await?;

    if folders.iter().any(|f| f["path"].as_str() == Some(cfg.root_path)) {
        info!("{label}: already exists");
        return Ok(());
    }

    upsert(
        &client,
        &base_url,
        "rootfolder",
        &api_key,
        None,
        &json!({"path": cfg.root_path}),
        &label,
    )
    .await
}

/// Add a Sonarr or Radarr instance as an application in Prowlarr for indexer sync.
pub async fn configure_prowlarr_app_sync(
    data_dir: &Path,
    prowlarr_id: &str,
    arr_id: &str,
    arr_type: &str,
) -> Result<(), AppError> {
    let prowlarr_state = config::load_app_state(data_dir, prowlarr_id)?;
    let arr_state = config::load_app_state(data_dir, arr_id)?;

    let prowlarr_port = prowlarr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port for '{prowlarr_id}'")))?;

    let prowlarr_key = get_arr_api_key(data_dir, prowlarr_id).await?;
    let arr_key = get_arr_api_key(data_dir, arr_id).await?;

    let prowlarr_host = container_host(data_dir, prowlarr_id, &prowlarr_state);
    let arr_host = container_host(data_dir, arr_id, &arr_state);
    let arr_port = internal_port_for(arr_type);

    let (app_name, implementation, config_contract) = match arr_type {
        "sonarr" => ("Sonarr (MyGround)", "Sonarr", "SonarrSettings"),
        "radarr" => ("Radarr (MyGround)", "Radarr", "RadarrSettings"),
        other => return Err(AppError::Io(format!("Unknown arr type: '{other}'"))),
    };

    let client = http_client(false)?;
    let base_url = format!("http://localhost:{prowlarr_port}/api/v1");
    let label = format!("Prowlarr app sync: {prowlarr_id} → {arr_id}");

    let apps = list_arr_entries(&client, &base_url, "applications", &prowlarr_key, &label).await?;
    let existing_id = find_existing_by_name(&apps, app_name);

    let payload = json!({
        "name": app_name,
        "implementation": implementation,
        "configContract": config_contract,
        "syncLevel": "fullSync",
        "tags": [],
        "fields": [
            {"name": "prowlarrUrl", "value": format!("http://{prowlarr_host}:{}", internal_port_for("prowlarr"))},
            {"name": "baseUrl",     "value": format!("http://{arr_host}:{arr_port}")},
            {"name": "apiKey",      "value": arr_key},
            {"name": "syncCategories", "value": [2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060]}
        ]
    });

    upsert(&client, &base_url, "applications", &prowlarr_key, existing_id, &payload, &label).await
}

/// Create download categories in qBittorrent for Sonarr and Radarr.
pub async fn configure_qbittorrent_categories(
    data_dir: &Path,
    qbt_id: &str,
) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, qbt_id)?;
    let port = state.port.ok_or_else(|| AppError::Io(format!("No port for '{qbt_id}'")))?;

    let user = state.env_overrides.get("QB_USERNAME").cloned().unwrap_or_else(|| "admin".into());
    let pass = state.env_overrides.get("QB_PASSWORD").cloned().unwrap_or_else(|| "adminadmin".into());

    let client = http_client(true)?;
    let base = format!("http://localhost:{port}/api/v2");

    // Authenticate
    let login_body = client
        .post(format!("{base}/auth/login"))
        .form(&[("username", user.as_str()), ("password", pass.as_str())])
        .send()
        .await
        .map_err(|e| AppError::Io(format!("qBittorrent login: {e}")))?
        .text()
        .await
        .unwrap_or_default();

    if login_body.trim() != "Ok." {
        return Err(AppError::Io(format!("qBittorrent login failed: {login_body}")));
    }

    // Create categories
    for (cat, path) in [("tv-sonarr", "/downloads/tv"), ("movies-radarr", "/downloads/movies")] {
        let status = client
            .post(format!("{base}/torrents/createCategory"))
            .form(&[("category", cat), ("savePath", path)])
            .send()
            .await
            .map_err(|e| AppError::Io(format!("qBittorrent createCategory '{cat}': {e}")))?
            .status();

        match status.as_u16() {
            200..=299 => info!("Created qBittorrent category '{cat}' → {path}"),
            409 => info!("qBittorrent category '{cat}' already exists"),
            _ => warn!("qBittorrent createCategory '{cat}' returned {status}"),
        }
    }

    Ok(())
}

// ── Orchestration ───────────────────────────────────────────────────────────

/// Process all `app_links` declared in an app's state.
///
/// Does NOT wait for health — callers should call [`wait_for_app_ready`] first.
async fn autoconfigure_links(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, app_id)?;

    if !state.installed || state.app_links.is_empty() {
        return Ok(());
    }

    let source_type = def_id(app_id, &state).to_string();

    for link in &state.app_links {
        let target_id = &link.target_id;

        let target_state = match config::load_app_state(data_dir, target_id) {
            Ok(s) if s.installed => s,
            Ok(_) => { warn!("Link target '{target_id}' not installed — skipping"); continue; }
            Err(e) => { warn!("Failed to load '{target_id}': {e} — skipping"); continue; }
        };
        let target_type = def_id(target_id, &target_state).to_string();

        match &link.link_type {
            config::LinkType::DownloadClient => {
                if let Some(cfg) = arr_config_for(&source_type) {
                    if let Err(e) = configure_arr_download_client(data_dir, app_id, target_id, cfg).await {
                        warn!("Download client config failed ({app_id} → {target_id}): {e}");
                    } else {
                        // Also set up root folder and qBittorrent categories
                        let _ = configure_arr_root_folder(data_dir, app_id, cfg)
                            .await
                            .map_err(|e| warn!("Root folder config failed for {app_id}: {e}"));
                        let _ = configure_qbittorrent_categories(data_dir, target_id)
                            .await
                            .map_err(|e| warn!("qBittorrent categories failed for {target_id}: {e}"));
                    }
                } else {
                    warn!("{app_id} (type {source_type}) has download_client link but is not sonarr/radarr");
                }
            }

            config::LinkType::Indexer => {
                // Determine which side is Prowlarr
                let (prowlarr, arr, arr_type) = if source_type == "prowlarr" {
                    (app_id, target_id.as_str(), target_type.as_str())
                } else if target_type == "prowlarr" {
                    (target_id.as_str(), app_id, source_type.as_str())
                } else {
                    warn!("Indexer link {app_id} ↔ {target_id}: neither is prowlarr");
                    continue;
                };

                if let Err(e) = configure_prowlarr_app_sync(data_dir, prowlarr, arr, arr_type).await {
                    warn!("Prowlarr app sync ({prowlarr} ↔ {arr}) failed: {e}");
                }
            }

            config::LinkType::MediaServer => {
                // Path-based only — no runtime API configuration needed.
            }
        }
    }

    Ok(())
}

/// Wait for an app to become healthy, then configure all its links.
///
/// For Prowlarr, also handles reverse links (other apps pointing to this
/// Prowlarr instance).
pub async fn run_autoconfigure(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, app_id)?;
    if !state.installed {
        return Ok(());
    }

    let app_type = def_id(app_id, &state).to_string();

    // Early exit if nothing to configure (unless Prowlarr — others may link to it)
    if state.app_links.is_empty() && app_type != "prowlarr" {
        return Ok(());
    }

    let port = match state.port {
        Some(p) => p,
        None => { warn!("'{app_id}' has no port — skipping autoconfigure"); return Ok(()); }
    };

    // Wait for health (up to 5 minutes)
    if let Err(e) = wait_for_app_ready(port, health_path_for(&app_type), 300).await {
        warn!("'{app_id}' health check failed ({e}); trying config anyway");
    }

    // Configure direct links
    if let Err(e) = autoconfigure_links(data_dir, app_id).await {
        warn!("autoconfigure_links failed for '{app_id}': {e}");
    }

    // Prowlarr: handle apps whose Indexer links point TO this instance
    if app_type == "prowlarr" {
        for (other_id, other_state) in config::list_installed_apps_with_state(data_dir) {
            if other_id == app_id { continue; }
            let other_type = def_id(&other_id, &other_state);
            for link in &other_state.app_links {
                if link.target_id == app_id && link.link_type == config::LinkType::Indexer {
                    if let Err(e) = configure_prowlarr_app_sync(data_dir, app_id, &other_id, other_type).await {
                        warn!("Prowlarr reverse sync ({app_id} ← {other_id}) failed: {e}");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Reconfigure all apps linked to or from `app_id`.
pub async fn autoconfigure_all_linked(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let app_state = config::load_app_state(data_dir, app_id).unwrap_or_default();
    let all = config::list_installed_apps_with_state(data_dir);

    let mut to_configure = vec![app_id.to_string()];

    for (other_id, other_state) in &all {
        if other_id == app_id { continue; }

        let linked = other_state.app_links.iter().any(|l| l.target_id == app_id)
            || app_state.app_links.iter().any(|l| l.target_id == *other_id);

        if linked && !to_configure.contains(other_id) {
            to_configure.push(other_id.clone());
        }
    }

    for id in &to_configure {
        if let Err(e) = run_autoconfigure(data_dir, id).await {
            warn!("autoconfigure_all_linked: '{id}' failed: {e}");
        }
    }

    Ok(())
}
