use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth;
use crate::config::{self, AuthConfig, TailscaleConfig};
use crate::state::AppState;

use super::response::{action_err, action_ok};

// ── API Key Types ────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub ok: bool,
    pub id: String,
    pub name: String,
    pub key: String,
}

#[derive(Serialize, ToSchema)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

// ── Constants ────────────────────────────────────────────────────────────────

const MIN_PASSWORD_LEN: usize = 8;
const MAX_API_KEYS: usize = 25;
const SESSION_MAX_AGE: u64 = 604_800;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AuthStatus {
    pub setup_required: bool,
    pub authenticated: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale_key: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub ok: bool,
    pub message: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn set_session_cookie(token: &str) -> axum::http::HeaderValue {
    let name = auth::SESSION_COOKIE_NAME;
    axum::http::HeaderValue::from_str(&format!(
        "{name}={token}; HttpOnly; Secure; Path=/; SameSite=Strict; Max-Age={SESSION_MAX_AGE}"
    ))
    .unwrap()
}

fn create_session(state: &AppState) -> String {
    let token = auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    token
}

// ── Endpoints ───────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/auth/status",
    responses(
        (status = 200, description = "Auth status", body = AuthStatus)
    )
)]
pub async fn auth_status(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
) -> Json<AuthStatus> {
    let auth_config = config::try_load_auth(&state.data_dir);
    let setup_required = auth_config.is_none();

    let authenticated = if setup_required {
        false
    } else {
        // Check session cookie
        req.headers()
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(auth::extract_session_from_cookies)
            .map(|token| state.sessions.read().unwrap().contains(token))
            .unwrap_or(false)
    };

    Json(AuthStatus {
        setup_required,
        authenticated,
    })
}

#[utoipa::path(
    post,
    path = "/auth/setup",
    request_body = SetupRequest,
    responses(
        (status = 200, description = "Setup complete", body = LoginResponse),
        (status = 400, description = "Setup error", body = LoginResponse)
    )
)]
pub async fn auth_setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> impl IntoResponse {
    // Only allow setup when no auth is configured
    if config::try_load_auth(&state.data_dir).is_some() {
        return action_err(StatusCode::BAD_REQUEST, "Already set up".to_string()).into_response();
    }

    if body.username.trim().is_empty() {
        return action_err(StatusCode::BAD_REQUEST, "Username required".to_string())
            .into_response();
    }

    if body.password.len() < MIN_PASSWORD_LEN {
        return action_err(
            StatusCode::BAD_REQUEST,
            format!("Password must be at least {MIN_PASSWORD_LEN} characters"),
        )
        .into_response();
    }

    // Hash password and save auth config
    let password_hash = match auth::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => {
            return action_err(StatusCode::BAD_REQUEST, format!("Hash error: {e}")).into_response()
        }
    };

    let auth_cfg = AuthConfig {
        username: body.username.trim().to_string(),
        password_hash,
        cli_token_hash: None,
        api_keys: vec![],
    };
    if let Err(e) = config::save_auth_config(&state.data_dir, &auth_cfg) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    // Optionally configure Tailscale and start exit node
    if let Some(ref ts_key) = body.tailscale_key {
        if !ts_key.trim().is_empty() {
            let ts_cfg = TailscaleConfig {
                enabled: true,
                auth_key: None, // Not stored — one-time use
                tailnet: None,
            };
            let _ = config::save_tailscale_config(&state.data_dir, &ts_cfg);
            // Cache key in memory for future service installs
            *state.tailscale_key.write().unwrap() = Some(ts_key.trim().to_string());
            if let Err(e) =
                crate::tailscale::ensure_exit_node(&state.data_dir, Some(ts_key.trim())).await
            {
                tracing::warn!("Failed to start exit node during setup: {e}");
            }
        }
    }

    // Create session and return cookie
    let token = create_session(&state);
    let mut response = Json(LoginResponse {
        ok: true,
        message: "Setup complete".to_string(),
    })
    .into_response();
    response
        .headers_mut()
        .insert("set-cookie", set_session_cookie(&token));
    response
}

#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = LoginResponse),
        (status = 429, description = "Too many attempts", body = LoginResponse)
    )
)]
pub async fn auth_login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    // Normalize username for consistent rate limiting (prevent case-variation bypass)
    let normalized = body.username.trim().to_lowercase();

    // Rate limiting: check if this username is blocked
    if state.login_attempts.read().unwrap().is_blocked(&normalized) {
        return action_err(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many failed attempts. Try again later.".to_string(),
        )
        .into_response();
    }

    let auth_config = match config::try_load_auth(&state.data_dir) {
        Some(c) => c,
        None => {
            return action_err(StatusCode::BAD_REQUEST, "Not set up yet".to_string()).into_response()
        }
    };

    if normalized != auth_config.username.to_lowercase()
        || !auth::verify_password(&body.password, &auth_config.password_hash)
    {
        // Record failed attempt against normalized username
        state
            .login_attempts
            .write()
            .unwrap()
            .record_failure(&normalized);
        return action_err(StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())
            .into_response();
    }

    // Clear rate limit on success
    state.login_attempts.write().unwrap().clear(&normalized);

    let token = create_session(&state);
    let mut response = Json(LoginResponse {
        ok: true,
        message: "Logged in".to_string(),
    })
    .into_response();
    response
        .headers_mut()
        .insert("set-cookie", set_session_cookie(&token));
    response
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    responses(
        (status = 200, description = "Logged out", body = LoginResponse)
    )
)]
pub async fn auth_logout(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    // Remove session
    if let Some(token) = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(auth::extract_session_from_cookies)
    {
        state.sessions.write().unwrap().remove(token);
    }

    let mut response = action_ok("Logged out".to_string()).into_response();
    let name = auth::SESSION_COOKIE_NAME;
    response.headers_mut().insert(
        "set-cookie",
        axum::http::HeaderValue::from_str(&format!(
            "{name}=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0"
        ))
        .unwrap(),
    );
    response
}

// ── API Key Endpoints ────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/auth/api-keys",
    responses(
        (status = 200, description = "List API keys", body = Vec<ApiKeyInfo>)
    )
)]
pub async fn api_keys_list(State(state): State<AppState>) -> Json<Vec<ApiKeyInfo>> {
    let auth_config = config::try_load_auth(&state.data_dir);
    let keys = auth_config
        .map(|c| {
            c.api_keys
                .into_iter()
                .map(|k| ApiKeyInfo {
                    id: k.id,
                    name: k.name,
                    created_at: k.created_at,
                })
                .collect()
        })
        .unwrap_or_default();
    Json(keys)
}

#[utoipa::path(
    post,
    path = "/auth/api-keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created", body = CreateApiKeyResponse),
        (status = 400, description = "Validation error", body = LoginResponse)
    )
)]
pub async fn api_keys_create(
    State(state): State<AppState>,
    Json(body): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    if body.name.trim().is_empty() {
        return action_err(StatusCode::BAD_REQUEST, "Name required".to_string()).into_response();
    }

    let mut auth_config = match config::try_load_auth(&state.data_dir) {
        Some(c) => c,
        None => {
            return action_err(StatusCode::BAD_REQUEST, "Not set up yet".to_string()).into_response()
        }
    };

    if auth_config.api_keys.len() >= MAX_API_KEYS {
        return action_err(
            StatusCode::BAD_REQUEST,
            format!("Maximum of {MAX_API_KEYS} API keys reached"),
        )
        .into_response();
    }

    let id = config::generate_key_id();
    let raw_key = format!("myground_ak_{}", auth::generate_session_token());
    let key_hash = match auth::hash_password(&raw_key) {
        Ok(h) => h,
        Err(e) => {
            return action_err(StatusCode::BAD_REQUEST, format!("Hash error: {e}")).into_response()
        }
    };

    let entry = config::ApiKeyEntry {
        id: id.clone(),
        name: body.name.trim().to_string(),
        key_hash,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    auth_config.api_keys.push(entry);

    if let Err(e) = config::save_auth_config(&state.data_dir, &auth_config) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    Json(CreateApiKeyResponse {
        ok: true,
        id,
        name: body.name.trim().to_string(),
        key: raw_key,
    })
    .into_response()
}

#[utoipa::path(
    delete,
    path = "/auth/api-keys/{id}",
    params(("id" = String, Path, description = "API key ID")),
    responses(
        (status = 200, description = "API key revoked", body = LoginResponse),
        (status = 404, description = "Key not found", body = LoginResponse)
    )
)]
pub async fn api_keys_revoke(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut auth_config = match config::try_load_auth(&state.data_dir) {
        Some(c) => c,
        None => {
            return action_err(StatusCode::BAD_REQUEST, "Not set up yet".to_string()).into_response()
        }
    };

    let before = auth_config.api_keys.len();
    auth_config.api_keys.retain(|k| k.id != id);

    if auth_config.api_keys.len() == before {
        return action_err(StatusCode::NOT_FOUND, format!("API key '{id}' not found")).into_response();
    }

    if let Err(e) = config::save_auth_config(&state.data_dir, &auth_config) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    action_ok("API key revoked".to_string()).into_response()
}
