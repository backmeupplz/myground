//! Auto-configuration engine for the *arr stack.
//!
//! Wires up Sonarr, Radarr, Prowlarr, and qBittorrent after containers start.
//! Called when links are added/changed or when an app with links is started.

use std::path::Path;

use serde_json::json;
use tracing::{info, warn};

use crate::config;
use crate::error::AppError;

// ── Internal Docker ports ───────────────────────────────────────────────────

/// Sonarr internal container port.
const SONARR_PORT: u16 = 8989;
/// Radarr internal container port.
const RADARR_PORT: u16 = 7878;
/// Prowlarr internal container port.
const PROWLARR_PORT: u16 = 9696;
/// qBittorrent internal container port.
const QBITTORRENT_PORT: u16 = 8080;

// ── Helper utilities ────────────────────────────────────────────────────────

/// Returns the host/container name by which an app is reachable on the shared
/// Docker network.  When a VPN sidecar is active, qBittorrent's process runs
/// inside the gluetun network namespace, so other containers must address it
/// via the VPN container name.
fn container_host(app_id: &str, state: &config::InstalledAppState) -> String {
    let vpn_enabled = state.vpn.as_ref().map(|v| v.enabled).unwrap_or(false);
    if vpn_enabled {
        format!("myground-{app_id}-vpn")
    } else {
        format!("myground-{app_id}")
    }
}

/// Returns the base definition ID (app type) for an instance.
///
/// For the first instance ("sonarr") `definition_id` is `None`, so we return
/// the `app_id` itself.  For subsequent instances ("sonarr-2") `definition_id`
/// is `Some("sonarr")`.
fn def_id<'a>(app_id: &'a str, state: &'a config::InstalledAppState) -> &'a str {
    state.definition_id.as_deref().unwrap_or(app_id)
}

/// Extract the text content of a simple XML element by tag name.
///
/// ```text
/// extract_xml_tag("<Config><ApiKey>abc123</ApiKey></Config>", "ApiKey")
/// // → Some("abc123")
/// ```
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

// ── Core functions ──────────────────────────────────────────────────────────

/// Poll an app's health endpoint until it returns HTTP 200 (or 401, which
/// signals that the app is up but requires authentication).
///
/// Returns `Ok(())` as soon as the app is reachable, or an error after
/// `timeout_secs` have elapsed.
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

    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // 200 = healthy, 401 = app running but auth required
                if resp.status().is_success() || status == 401 {
                    return Ok(());
                }
            }
            Err(_) => {}
        }

        if std::time::Instant::now() >= deadline {
            return Err(AppError::Io(format!(
                "App on port {port} did not become ready within {timeout_secs}s"
            )));
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

/// Read the API key for a Sonarr / Radarr / Prowlarr instance.
///
/// Each *arr app writes its auto-generated API key to
/// `{config_volume}/config.xml` on first start.  This function retries up to
/// 10 times with 3-second delays to handle the race between container startup
/// and the file appearing on disk.
pub async fn get_arr_api_key(data_dir: &Path, app_id: &str) -> Result<String, AppError> {
    let state = config::load_app_state(data_dir, app_id)?;

    let config_path = state
        .storage_paths
        .get("config")
        .cloned()
        .ok_or_else(|| {
            AppError::Io(format!(
                "App '{app_id}' has no 'config' storage path — cannot read config.xml"
            ))
        })?;

    let xml_path = std::path::PathBuf::from(&config_path).join("config.xml");

    for attempt in 0..10u32 {
        if xml_path.exists() {
            let content = std::fs::read_to_string(&xml_path).map_err(|e| {
                AppError::Io(format!("Read {}: {e}", xml_path.display()))
            })?;

            if let Some(key) = extract_xml_tag(&content, "ApiKey") {
                return Ok(key);
            }
        }

        if attempt < 9 {
            tracing::debug!(
                "config.xml not ready for {app_id} (attempt {}/10), retrying in 3s…",
                attempt + 1
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    }

    Err(AppError::Io(format!(
        "Could not read API key for '{app_id}': config.xml missing or ApiKey absent after retries"
    )))
}

// ── Per-app configuration functions ────────────────────────────────────────

/// Configure qBittorrent as the download client inside **Sonarr**.
///
/// Creates (or updates) a "qBittorrent (MyGround)" download-client entry via
/// the Sonarr v3 REST API.  The host address used in the Sonarr config is the
/// Docker-internal container name, so Sonarr reaches qBittorrent on the shared
/// Docker network without going through the host.
pub async fn configure_sonarr_download_client(
    data_dir: &Path,
    sonarr_id: &str,
    qbt_id: &str,
) -> Result<(), AppError> {
    let sonarr_state = config::load_app_state(data_dir, sonarr_id)?;
    let qbt_state = config::load_app_state(data_dir, qbt_id)?;

    let sonarr_port = sonarr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{sonarr_id}'")))?;

    let api_key = get_arr_api_key(data_dir, sonarr_id).await?;

    let qbt_host = container_host(qbt_id, &qbt_state);
    let qbt_username = qbt_state
        .env_overrides
        .get("QB_USERNAME")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());
    let qbt_password = qbt_state
        .env_overrides
        .get("QB_PASSWORD")
        .cloned()
        .unwrap_or_else(|| "adminadmin".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{sonarr_port}/api/v3");

    // Check whether a matching download client already exists
    let list_resp = client
        .get(format!("{base_url}/downloadclient"))
        .header("X-Api-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Sonarr /downloadclient GET failed: {e}")))?;

    if !list_resp.status().is_success() {
        return Err(AppError::Io(format!(
            "Sonarr API ({sonarr_id}) returned {} listing download clients",
            list_resp.status()
        )));
    }

    let clients: Vec<serde_json::Value> = list_resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse Sonarr download clients: {e}")))?;

    let existing_id = clients.iter().find_map(|c| {
        if c["name"].as_str() == Some("qBittorrent (MyGround)") {
            c["id"].as_i64()
        } else {
            None
        }
    });

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
            {"name": "port", "value": QBITTORRENT_PORT},
            {"name": "username", "value": qbt_username},
            {"name": "password", "value": qbt_password},
            {"name": "tvCategory", "value": "tv-sonarr"},
            {"name": "recentTvPriority", "value": 0},
            {"name": "olderTvPriority", "value": 0},
            {"name": "initialState", "value": 0},
            {"name": "sequentialOrder", "value": false},
            {"name": "firstAndLast", "value": false}
        ]
    });

    let resp = if let Some(id) = existing_id {
        client
            .put(format!("{base_url}/downloadclient/{id}"))
            .header("X-Api-Key", &api_key)
            .json(&payload)
            .send()
            .await
    } else {
        client
            .post(format!("{base_url}/downloadclient"))
            .header("X-Api-Key", &api_key)
            .json(&payload)
            .send()
            .await
    }
    .map_err(|e| AppError::Io(format!("Sonarr /downloadclient request failed: {e}")))?;

    if resp.status().is_success() {
        info!(
            "Configured qBittorrent ({qbt_id}) as download client in Sonarr ({sonarr_id})"
        );
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "Sonarr download-client config failed ({status}): {body}"
        )))
    }
}

/// Configure qBittorrent as the download client inside **Radarr**.
///
/// Same as [`configure_sonarr_download_client`] but uses the Radarr API and
/// `movieCategory` / `movies-radarr` instead of `tvCategory` / `tv-sonarr`.
pub async fn configure_radarr_download_client(
    data_dir: &Path,
    radarr_id: &str,
    qbt_id: &str,
) -> Result<(), AppError> {
    let radarr_state = config::load_app_state(data_dir, radarr_id)?;
    let qbt_state = config::load_app_state(data_dir, qbt_id)?;

    let radarr_port = radarr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{radarr_id}'")))?;

    let api_key = get_arr_api_key(data_dir, radarr_id).await?;

    let qbt_host = container_host(qbt_id, &qbt_state);
    let qbt_username = qbt_state
        .env_overrides
        .get("QB_USERNAME")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());
    let qbt_password = qbt_state
        .env_overrides
        .get("QB_PASSWORD")
        .cloned()
        .unwrap_or_else(|| "adminadmin".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{radarr_port}/api/v3");

    let list_resp = client
        .get(format!("{base_url}/downloadclient"))
        .header("X-Api-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Radarr /downloadclient GET failed: {e}")))?;

    if !list_resp.status().is_success() {
        return Err(AppError::Io(format!(
            "Radarr API ({radarr_id}) returned {} listing download clients",
            list_resp.status()
        )));
    }

    let clients: Vec<serde_json::Value> = list_resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse Radarr download clients: {e}")))?;

    let existing_id = clients.iter().find_map(|c| {
        if c["name"].as_str() == Some("qBittorrent (MyGround)") {
            c["id"].as_i64()
        } else {
            None
        }
    });

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
            {"name": "port", "value": QBITTORRENT_PORT},
            {"name": "username", "value": qbt_username},
            {"name": "password", "value": qbt_password},
            {"name": "movieCategory", "value": "movies-radarr"},
            {"name": "recentMoviePriority", "value": 0},
            {"name": "olderMoviePriority", "value": 0},
            {"name": "initialState", "value": 0},
            {"name": "sequentialOrder", "value": false},
            {"name": "firstAndLast", "value": false}
        ]
    });

    let resp = if let Some(id) = existing_id {
        client
            .put(format!("{base_url}/downloadclient/{id}"))
            .header("X-Api-Key", &api_key)
            .json(&payload)
            .send()
            .await
    } else {
        client
            .post(format!("{base_url}/downloadclient"))
            .header("X-Api-Key", &api_key)
            .json(&payload)
            .send()
            .await
    }
    .map_err(|e| AppError::Io(format!("Radarr /downloadclient request failed: {e}")))?;

    if resp.status().is_success() {
        info!(
            "Configured qBittorrent ({qbt_id}) as download client in Radarr ({radarr_id})"
        );
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "Radarr download-client config failed ({status}): {body}"
        )))
    }
}

/// Ensure `/tv` exists as a root folder in **Sonarr**.
///
/// `/tv` is the path where the `TV_PATH` host directory is mounted inside the
/// Sonarr container.  If the root folder already exists, this is a no-op.
pub async fn configure_sonarr_root_folder(
    data_dir: &Path,
    sonarr_id: &str,
) -> Result<(), AppError> {
    let sonarr_state = config::load_app_state(data_dir, sonarr_id)?;
    let sonarr_port = sonarr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{sonarr_id}'")))?;
    let api_key = get_arr_api_key(data_dir, sonarr_id).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{sonarr_port}/api/v3");

    let list_resp = client
        .get(format!("{base_url}/rootfolder"))
        .header("X-Api-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Sonarr /rootfolder GET failed: {e}")))?;

    if !list_resp.status().is_success() {
        return Err(AppError::Io(format!(
            "Sonarr API ({sonarr_id}) returned {} listing root folders",
            list_resp.status()
        )));
    }

    let folders: Vec<serde_json::Value> = list_resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse Sonarr root folders: {e}")))?;

    if folders.iter().any(|f| f["path"].as_str() == Some("/tv")) {
        info!("Sonarr ({sonarr_id}) already has /tv root folder — skipping");
        return Ok(());
    }

    let resp = client
        .post(format!("{base_url}/rootfolder"))
        .header("X-Api-Key", &api_key)
        .json(&json!({"path": "/tv"}))
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Sonarr /rootfolder POST failed: {e}")))?;

    if resp.status().is_success() {
        info!("Added /tv root folder to Sonarr ({sonarr_id})");
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "Sonarr root-folder config failed ({status}): {body}"
        )))
    }
}

/// Ensure `/movies` exists as a root folder in **Radarr**.
///
/// `/movies` is the path where the `MOVIES_PATH` host directory is mounted
/// inside the Radarr container.  If the root folder already exists, this is a
/// no-op.
pub async fn configure_radarr_root_folder(
    data_dir: &Path,
    radarr_id: &str,
) -> Result<(), AppError> {
    let radarr_state = config::load_app_state(data_dir, radarr_id)?;
    let radarr_port = radarr_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{radarr_id}'")))?;
    let api_key = get_arr_api_key(data_dir, radarr_id).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{radarr_port}/api/v3");

    let list_resp = client
        .get(format!("{base_url}/rootfolder"))
        .header("X-Api-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Radarr /rootfolder GET failed: {e}")))?;

    if !list_resp.status().is_success() {
        return Err(AppError::Io(format!(
            "Radarr API ({radarr_id}) returned {} listing root folders",
            list_resp.status()
        )));
    }

    let folders: Vec<serde_json::Value> = list_resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse Radarr root folders: {e}")))?;

    if folders.iter().any(|f| f["path"].as_str() == Some("/movies")) {
        info!("Radarr ({radarr_id}) already has /movies root folder — skipping");
        return Ok(());
    }

    let resp = client
        .post(format!("{base_url}/rootfolder"))
        .header("X-Api-Key", &api_key)
        .json(&json!({"path": "/movies"}))
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Radarr /rootfolder POST failed: {e}")))?;

    if resp.status().is_success() {
        info!("Added /movies root folder to Radarr ({radarr_id})");
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "Radarr root-folder config failed ({status}): {body}"
        )))
    }
}

/// Add a Sonarr or Radarr instance as an **application** in Prowlarr so that
/// indexers are automatically synced.
///
/// # Arguments
///
/// * `prowlarr_id` — instance ID of the Prowlarr app (e.g. `"prowlarr"` or
///   `"prowlarr-2"`)
/// * `arr_id`      — instance ID of the arr app (e.g. `"sonarr"`, `"radarr-2"`)
/// * `arr_type`    — base app type: `"sonarr"` or `"radarr"`
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
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{prowlarr_id}'")))?;

    let prowlarr_api_key = get_arr_api_key(data_dir, prowlarr_id).await?;
    let arr_api_key = get_arr_api_key(data_dir, arr_id).await?;

    // URL that Prowlarr uses to reach itself (from inside the Sonarr/Radarr containers)
    let prowlarr_container = container_host(prowlarr_id, &prowlarr_state);
    let prowlarr_url = format!("http://{prowlarr_container}:{PROWLARR_PORT}");

    // URL that Prowlarr uses to reach the arr app
    let arr_container = container_host(arr_id, &arr_state);

    let (app_name, implementation, config_contract, arr_base_url) = match arr_type {
        "sonarr" => (
            "Sonarr (MyGround)",
            "Sonarr",
            "SonarrSettings",
            format!("http://{arr_container}:{SONARR_PORT}"),
        ),
        "radarr" => (
            "Radarr (MyGround)",
            "Radarr",
            "RadarrSettings",
            format!("http://{arr_container}:{RADARR_PORT}"),
        ),
        other => {
            return Err(AppError::Io(format!(
                "configure_prowlarr_app_sync: unknown arr_type '{other}'"
            )));
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{prowlarr_port}/api/v1");

    // Check whether this application is already registered
    let list_resp = client
        .get(format!("{base_url}/applications"))
        .header("X-Api-Key", &prowlarr_api_key)
        .send()
        .await
        .map_err(|e| AppError::Io(format!("Prowlarr /applications GET failed: {e}")))?;

    if !list_resp.status().is_success() {
        return Err(AppError::Io(format!(
            "Prowlarr API ({prowlarr_id}) returned {} listing applications",
            list_resp.status()
        )));
    }

    let apps: Vec<serde_json::Value> = list_resp
        .json()
        .await
        .map_err(|e| AppError::Io(format!("Parse Prowlarr applications: {e}")))?;

    let existing_id = apps.iter().find_map(|a| {
        if a["name"].as_str() == Some(app_name) {
            a["id"].as_i64()
        } else {
            None
        }
    });

    let payload = json!({
        "name": app_name,
        "implementation": implementation,
        "configContract": config_contract,
        "syncLevel": "fullSync",
        "tags": [],
        "fields": [
            {"name": "prowlarrUrl", "value": prowlarr_url},
            {"name": "baseUrl",     "value": arr_base_url},
            {"name": "apiKey",      "value": arr_api_key},
            {"name": "syncCategories", "value": [2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060]}
        ]
    });

    let resp = if let Some(id) = existing_id {
        client
            .put(format!("{base_url}/applications/{id}"))
            .header("X-Api-Key", &prowlarr_api_key)
            .json(&payload)
            .send()
            .await
    } else {
        client
            .post(format!("{base_url}/applications"))
            .header("X-Api-Key", &prowlarr_api_key)
            .json(&payload)
            .send()
            .await
    }
    .map_err(|e| AppError::Io(format!("Prowlarr /applications request failed: {e}")))?;

    if resp.status().is_success() {
        info!(
            "Configured {arr_type} ({arr_id}) as application in Prowlarr ({prowlarr_id})"
        );
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::Io(format!(
            "Prowlarr app-sync config failed ({status}): {body}"
        )))
    }
}

/// Create `tv-sonarr` and `movies-radarr` download categories in **qBittorrent**.
///
/// These categories tell qBittorrent where to save torrents dispatched by
/// Sonarr and Radarr respectively.  Existing categories are left unchanged
/// (a 409 response is treated as success).
pub async fn configure_qbittorrent_categories(
    data_dir: &Path,
    qbt_id: &str,
) -> Result<(), AppError> {
    let qbt_state = config::load_app_state(data_dir, qbt_id)?;
    let qbt_port = qbt_state
        .port
        .ok_or_else(|| AppError::Io(format!("No port allocated for '{qbt_id}'")))?;

    let username = qbt_state
        .env_overrides
        .get("QB_USERNAME")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());
    let password = qbt_state
        .env_overrides
        .get("QB_PASSWORD")
        .cloned()
        .unwrap_or_else(|| "adminadmin".to_string());

    // A cookie-jar client so the SID session cookie is kept across requests.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .cookie_store(true)
        .build()
        .map_err(|e| AppError::Io(format!("Build HTTP client: {e}")))?;

    let base_url = format!("http://localhost:{qbt_port}/api/v2");

    // Authenticate — qBittorrent returns the plain text "Ok." on success.
    let login_resp = client
        .post(format!("{base_url}/auth/login"))
        .form(&[("username", username.as_str()), ("password", password.as_str())])
        .send()
        .await
        .map_err(|e| AppError::Io(format!("qBittorrent login request failed: {e}")))?;

    let login_body = login_resp.text().await.unwrap_or_default();
    if login_body.trim() != "Ok." {
        return Err(AppError::Io(format!(
            "qBittorrent ({qbt_id}) login failed: {login_body}"
        )));
    }

    // Create the two categories required by Sonarr / Radarr.
    let categories: &[(&str, &str)] = &[
        ("tv-sonarr",     "/downloads/tv"),
        ("movies-radarr", "/downloads/movies"),
    ];

    for (category, save_path) in categories {
        let resp = client
            .post(format!("{base_url}/torrents/createCategory"))
            .form(&[("category", *category), ("savePath", *save_path)])
            .send()
            .await
            .map_err(|e| {
                AppError::Io(format!(
                    "qBittorrent createCategory '{category}' failed: {e}"
                ))
            })?;

        let status = resp.status();
        if status.is_success() {
            info!("Created qBittorrent category '{category}' → {save_path}");
        } else if status.as_u16() == 409 {
            // 409 Conflict = category already exists, not an error.
            info!("qBittorrent category '{category}' already exists — skipping");
        } else {
            warn!(
                "qBittorrent createCategory '{category}' returned unexpected status {status}"
            );
        }
    }

    Ok(())
}

// ── Orchestration helpers ───────────────────────────────────────────────────

/// Health-check URL paths for each app type.
fn health_path_for(def_id: &str) -> &'static str {
    match def_id {
        "sonarr"       => "/api/v3/health",
        "radarr"       => "/api/v3/health",
        "prowlarr"     => "/api/v1/health",
        "qbittorrent"  => "/api/v2/app/version",
        _              => "/",
    }
}

/// Process all `app_links` declared in an app's state and call the appropriate
/// configuration function for each one.
///
/// This function itself does **not** wait for the app to be healthy — callers
/// should call [`wait_for_app_ready`] first when that guarantee is needed.
pub async fn autoconfigure_links(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, app_id)?;

    if !state.installed || state.app_links.is_empty() {
        return Ok(());
    }

    let source_def = def_id(app_id, &state).to_string();

    for link in &state.app_links {
        let target_id = &link.target_id;

        // Validate that the target is actually installed.
        let target_state = match config::load_app_state(data_dir, target_id) {
            Ok(s) if s.installed => s,
            Ok(_) => {
                warn!(
                    "autoconfigure: link target '{target_id}' is not installed — skipping"
                );
                continue;
            }
            Err(e) => {
                warn!(
                    "autoconfigure: failed to load state for '{target_id}': {e} — skipping"
                );
                continue;
            }
        };
        let target_def = def_id(target_id, &target_state).to_string();

        match &link.link_type {
            // ── Sonarr/Radarr → qBittorrent ────────────────────────────────
            config::LinkType::DownloadClient => match source_def.as_str() {
                "sonarr" => {
                    if let Err(e) =
                        configure_sonarr_download_client(data_dir, app_id, target_id)
                            .await
                    {
                        warn!("configure_sonarr_download_client failed: {e}");
                    } else {
                        // Also set up root folder and qBittorrent categories
                        if let Err(e) =
                            configure_sonarr_root_folder(data_dir, app_id).await
                        {
                            warn!("configure_sonarr_root_folder failed: {e}");
                        }
                        if let Err(e) =
                            configure_qbittorrent_categories(data_dir, target_id).await
                        {
                            warn!("configure_qbittorrent_categories failed: {e}");
                        }
                    }
                }
                "radarr" => {
                    if let Err(e) =
                        configure_radarr_download_client(data_dir, app_id, target_id)
                            .await
                    {
                        warn!("configure_radarr_download_client failed: {e}");
                    } else {
                        if let Err(e) =
                            configure_radarr_root_folder(data_dir, app_id).await
                        {
                            warn!("configure_radarr_root_folder failed: {e}");
                        }
                        if let Err(e) =
                            configure_qbittorrent_categories(data_dir, target_id).await
                        {
                            warn!("configure_qbittorrent_categories failed: {e}");
                        }
                    }
                }
                other => {
                    warn!(
                        "autoconfigure: '{app_id}' (type {other}) has 'download_client' \
                         link but is not sonarr/radarr — skipping"
                    );
                }
            },

            // ── Prowlarr → Sonarr/Radarr (Indexer covers both app_sync and indexer_sync)
            config::LinkType::Indexer => match source_def.as_str() {
                "prowlarr" => {
                    if let Err(e) = configure_prowlarr_app_sync(
                        data_dir,
                        app_id,
                        target_id,
                        &target_def,
                    )
                    .await
                    {
                        warn!(
                            "configure_prowlarr_app_sync ({app_id} → {target_id}) \
                             failed: {e}"
                        );
                    }
                }
                // If source is not prowlarr, try reverse direction (arr → prowlarr)
                other => {
                    if target_def == "prowlarr" {
                        if let Err(e) = configure_prowlarr_app_sync(
                            data_dir,
                            target_id,
                            app_id,
                            &source_def,
                        )
                        .await
                        {
                            warn!(
                                "configure_prowlarr_app_sync ({target_id} ← {app_id}) \
                                 failed: {e}"
                            );
                        }
                    } else {
                        warn!(
                            "autoconfigure: '{app_id}' (type {other}) has 'indexer' link \
                             to '{target_id}' (type {target_def}) — neither is prowlarr, skipping"
                        );
                    }
                }
            },

            // ── MediaServer links are path-based, no auto-configuration needed ──
            config::LinkType::MediaServer => {
                // No runtime API configuration needed — just a shared filesystem path.
            }
        }
    }

    Ok(())
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Wait for an app to become healthy, then configure all of its declared links.
///
/// This is the primary entry point called after an app's containers are
/// started or restarted.
///
/// # Steps
///
/// 1. Load the app's [`InstalledAppState`][config::InstalledAppState].
/// 2. If the app has no links, return early.
/// 3. Wait up to 5 minutes for the app to respond to its health endpoint.
/// 4. Call [`autoconfigure_links`] to process every declared link.
/// 5. **Prowlarr special case**: scan all other installed apps for links that
///    *point to* this Prowlarr instance and configure those connections too.
pub async fn run_autoconfigure(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let state = config::load_app_state(data_dir, app_id)?;

    if !state.installed {
        return Ok(());
    }

    // Early exit if nothing to configure
    if state.app_links.is_empty() {
        // Even with no explicit links we still proceed for Prowlarr: other apps
        // might have indexer_sync links pointing *to* this Prowlarr.
        let source_def = def_id(app_id, &state).to_string();
        if source_def != "prowlarr" {
            return Ok(());
        }
    }

    let port = match state.port {
        Some(p) => p,
        None => {
            warn!(
                "run_autoconfigure: '{app_id}' has no port — cannot health-check, \
                 skipping autoconfigure"
            );
            return Ok(());
        }
    };

    let source_def = def_id(app_id, &state).to_string();
    let health = health_path_for(&source_def);

    // Wait for the app to be healthy (up to 5 minutes)
    if let Err(e) = wait_for_app_ready(port, health, 300).await {
        warn!(
            "run_autoconfigure: '{app_id}' health check failed ({e}); \
             attempting configuration anyway"
        );
    }

    // Configure all direct links declared on this app
    if let Err(e) = autoconfigure_links(data_dir, app_id).await {
        warn!("autoconfigure_links failed for '{app_id}': {e}");
    }

    // Prowlarr special case: also handle arr apps whose indexer_sync links
    // point *to* this Prowlarr instance.
    if source_def == "prowlarr" {
        let all = config::list_installed_apps_with_state(data_dir);
        for (other_id, other_state) in &all {
            if other_id == app_id {
                continue;
            }
            let other_def = def_id(other_id, other_state);
            for link in &other_state.app_links {
                if link.target_id == app_id
                    && link.link_type == config::LinkType::Indexer
                {
                    if let Err(e) =
                        configure_prowlarr_app_sync(data_dir, app_id, other_id, other_def)
                            .await
                    {
                        warn!(
                            "run_autoconfigure: configure_prowlarr_app_sync \
                             ({app_id} ↔ {other_id}) failed: {e}"
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Find every app that links **to** or is linked **from** `app_id`, then call
/// [`run_autoconfigure`] on each to bring all connections up-to-date.
///
/// This is useful after a link is added or an app is restarted so that the
/// entire dependent graph is reconfigured in one shot.
pub async fn autoconfigure_all_linked(data_dir: &Path, app_id: &str) -> Result<(), AppError> {
    let all = config::list_installed_apps_with_state(data_dir);

    // Load app_id's own links to detect outward connections.
    let app_state = config::load_app_state(data_dir, app_id).unwrap_or_default();

    let mut to_configure: Vec<String> = vec![app_id.to_string()];

    for (other_id, other_state) in &all {
        if other_id == app_id {
            continue;
        }

        // Other app has a link pointing TO app_id.
        let other_links_to_us = other_state.app_links.iter().any(|l| l.target_id == app_id);

        // app_id has a link pointing TO other_id.
        let we_link_to_other = app_state.app_links.iter().any(|l| l.target_id == *other_id);

        if (other_links_to_us || we_link_to_other) && !to_configure.contains(other_id) {
            to_configure.push(other_id.clone());
        }
    }

    for id in &to_configure {
        if let Err(e) = run_autoconfigure(data_dir, id).await {
            warn!("autoconfigure_all_linked: run_autoconfigure for '{id}' failed: {e}");
        }
    }

    Ok(())
}
