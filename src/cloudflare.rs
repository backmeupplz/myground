use std::path::Path;
use std::process::Stdio;

use base64::Engine;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, DomainBinding};
use crate::error::ServiceError;

const CLOUDFLARED_CONTAINER: &str = "myground-cloudflared";
const TUNNEL_NAME: &str = "myground";
const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";

// ── API response types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CfResponse<T> {
    success: bool,
    result: Option<T>,
    errors: Vec<CfError>,
}

#[derive(Debug, Deserialize)]
struct CfError {
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct CfAccount {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CfTunnel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CfZone {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CfDnsRecord {
    pub id: String,
}

// ── Cloudflare API client ───────────────────────────────────────────────────

pub struct CloudflareClient {
    client: reqwest::Client,
    api_token: String,
}

impl CloudflareClient {
    pub fn new(api_token: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_token: api_token.to_string(),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_token)
    }

    fn extract_result<T>(resp: CfResponse<T>) -> Result<T, ServiceError> {
        if !resp.success {
            let msg = resp
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ServiceError::Io(format!("Cloudflare API error: {msg}")));
        }
        resp.result
            .ok_or_else(|| ServiceError::Io("Cloudflare API returned empty result".to_string()))
    }

    pub async fn list_accounts(&self) -> Result<Vec<CfAccount>, ServiceError> {
        let resp: CfResponse<Vec<CfAccount>> = self
            .client
            .get(format!("{CF_API_BASE}/accounts"))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Cloudflare request failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("Cloudflare JSON parse: {e}")))?;
        Self::extract_result(resp)
    }

    pub async fn list_tunnels(&self, account_id: &str) -> Result<Vec<CfTunnel>, ServiceError> {
        let resp: CfResponse<Vec<CfTunnel>> = self
            .client
            .get(format!(
                "{CF_API_BASE}/accounts/{account_id}/cfd_tunnel?name={TUNNEL_NAME}&is_deleted=false"
            ))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("List tunnels failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("List tunnels JSON: {e}")))?;
        Self::extract_result(resp)
    }

    pub async fn create_tunnel(
        &self,
        account_id: &str,
    ) -> Result<(String, String), ServiceError> {
        // Generate a random 32-byte secret for the tunnel
        let mut secret_bytes = [0u8; 32];
        rand::Fill::fill(&mut secret_bytes, &mut rand::rng());
        let secret_b64 = base64::engine::general_purpose::STANDARD.encode(secret_bytes);

        let body = serde_json::json!({
            "name": TUNNEL_NAME,
            "tunnel_secret": secret_b64,
            "config_src": "cloudflare",
        });

        let resp: CfResponse<CfTunnel> = self
            .client
            .post(format!(
                "{CF_API_BASE}/accounts/{account_id}/cfd_tunnel"
            ))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Create tunnel failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("Create tunnel JSON: {e}")))?;

        let tunnel = Self::extract_result(resp)?;

        // Fetch the token for the newly created tunnel
        let token = self.get_tunnel_token(account_id, &tunnel.id).await?;
        Ok((tunnel.id, token))
    }

    pub async fn get_tunnel_token(
        &self,
        account_id: &str,
        tunnel_id: &str,
    ) -> Result<String, ServiceError> {
        let resp: CfResponse<String> = self
            .client
            .get(format!(
                "{CF_API_BASE}/accounts/{account_id}/cfd_tunnel/{tunnel_id}/token"
            ))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Get tunnel token failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("Get tunnel token JSON: {e}")))?;
        Self::extract_result(resp)
    }

    pub async fn list_zones(&self) -> Result<Vec<CfZone>, ServiceError> {
        let resp: CfResponse<Vec<CfZone>> = self
            .client
            .get(format!("{CF_API_BASE}/zones"))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("List zones failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("List zones JSON: {e}")))?;
        Self::extract_result(resp)
    }

    pub async fn create_cname(
        &self,
        zone_id: &str,
        fqdn: &str,
        tunnel_id: &str,
        proxied: bool,
    ) -> Result<String, ServiceError> {
        let body = serde_json::json!({
            "type": "CNAME",
            "name": fqdn,
            "content": format!("{tunnel_id}.cfargotunnel.com"),
            "proxied": proxied,
            "ttl": 1,
        });

        let resp: CfResponse<CfDnsRecord> = self
            .client
            .post(format!("{CF_API_BASE}/zones/{zone_id}/dns_records"))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Create CNAME failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Io(format!("Create CNAME JSON: {e}")))?;

        let record = Self::extract_result(resp)?;
        Ok(record.id)
    }

    pub async fn delete_dns_record(
        &self,
        zone_id: &str,
        record_id: &str,
    ) -> Result<(), ServiceError> {
        let resp = self
            .client
            .delete(format!(
                "{CF_API_BASE}/zones/{zone_id}/dns_records/{record_id}"
            ))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Delete DNS record failed: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ServiceError::Io(format!("Delete DNS record error: {body}")));
        }
        Ok(())
    }

    pub async fn update_tunnel_ingress(
        &self,
        account_id: &str,
        tunnel_id: &str,
        rules: Vec<IngressRule>,
    ) -> Result<(), ServiceError> {
        let body = serde_json::json!({
            "config": {
                "ingress": rules,
            }
        });

        let resp = self
            .client
            .put(format!(
                "{CF_API_BASE}/accounts/{account_id}/cfd_tunnel/{tunnel_id}/configurations"
            ))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::Io(format!("Update tunnel ingress failed: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ServiceError::Io(format!(
                "Update tunnel ingress error: {body}"
            )));
        }
        Ok(())
    }
}

// ── Ingress rules ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    pub service: String,
}

/// Build the FQDN for a domain binding.
pub fn build_fqdn(subdomain: &str, zone_name: &str) -> String {
    if subdomain.is_empty() {
        zone_name.to_string()
    } else {
        format!("{subdomain}.{zone_name}")
    }
}

/// Collect ingress rules from all services with domain bindings.
pub fn collect_ingress_rules(base: &Path) -> Vec<IngressRule> {
    let mut rules = Vec::new();

    for id in config::list_installed_services(base) {
        let state = match config::load_service_state(base, &id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let (Some(domain), Some(port)) = (&state.domain, state.port) {
            let fqdn = build_fqdn(&domain.subdomain, &domain.zone_name);
            rules.push(IngressRule {
                hostname: Some(fqdn),
                service: format!("http://localhost:{port}"),
            });
        }
    }

    // Mandatory catch-all
    rules.push(IngressRule {
        hostname: None,
        service: "http_status:404".to_string(),
    });

    rules
}

// ── Setup automation ────────────────────────────────────────────────────────

/// Full Cloudflare setup: detect account, find/create tunnel, save config, start cloudflared.
pub async fn setup_cloudflare(base: &Path, api_token: &str) -> Result<(), ServiceError> {
    let client = CloudflareClient::new(api_token);

    // 1. List accounts → pick first
    let accounts = client.list_accounts().await?;
    let account = accounts
        .first()
        .ok_or_else(|| ServiceError::Io("No Cloudflare accounts found. Check your API token permissions.".to_string()))?;
    let account_id = account.id.clone();

    // 2. Find or create tunnel
    let tunnels = client.list_tunnels(&account_id).await?;
    let (tunnel_id, tunnel_token) = if let Some(existing) = tunnels.first() {
        let token = client
            .get_tunnel_token(&account_id, &existing.id)
            .await?;
        (existing.id.clone(), token)
    } else {
        client.create_tunnel(&account_id).await?
    };

    // 3. Save config
    let cf_config = config::CloudflareConfig {
        enabled: true,
        api_token: Some(api_token.to_string()),
        account_id: Some(account_id),
        tunnel_id: Some(tunnel_id),
        tunnel_token: Some(tunnel_token.clone()),
    };
    config::save_cloudflare_config(base, &cf_config)?;

    // 4. Start cloudflared
    ensure_cloudflared(base, &tunnel_token).await?;

    // 5. Push current ingress rules (may be empty + catch-all)
    let rules = collect_ingress_rules(base);
    if let (Some(aid), Some(tid)) = (&cf_config.account_id, &cf_config.tunnel_id) {
        if let Err(e) = client.update_tunnel_ingress(aid, tid, rules).await {
            tracing::warn!("Failed to set initial ingress rules: {e}");
        }
    }

    Ok(())
}

// ── Container lifecycle ─────────────────────────────────────────────────────

fn generate_cloudflared_compose(tunnel_token: &str) -> String {
    format!(
        r#"services:
  cloudflared:
    image: cloudflare/cloudflared:latest
    container_name: {CLOUDFLARED_CONTAINER}
    network_mode: host
    environment:
      TUNNEL_TOKEN: "{tunnel_token}"
    command: tunnel run
    restart: unless-stopped
"#
    )
}

pub async fn ensure_cloudflared(base: &Path, tunnel_token: &str) -> Result<(), ServiceError> {
    let cf_dir = base.join("cloudflared");
    std::fs::create_dir_all(&cf_dir)
        .map_err(|e| ServiceError::Io(format!("Create cloudflared dir: {e}")))?;

    let compose = generate_cloudflared_compose(tunnel_token);
    std::fs::write(cf_dir.join("docker-compose.yml"), &compose)
        .map_err(|e| ServiceError::Io(format!("Write cloudflared compose: {e}")))?;

    let compose_cmd = crate::compose::detect_command().await?;
    crate::compose::run(&compose_cmd, &cf_dir, &["up", "-d"]).await?;

    Ok(())
}

pub async fn stop_cloudflared(base: &Path) -> Result<(), ServiceError> {
    let cf_dir = base.join("cloudflared");
    if !cf_dir.join("docker-compose.yml").exists() {
        return Ok(());
    }

    let compose_cmd = crate::compose::detect_command().await?;
    let _ = crate::compose::run(&compose_cmd, &cf_dir, &["down", "--remove-orphans"]).await;

    Ok(())
}

pub async fn is_cloudflared_running() -> bool {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", CLOUDFLARED_CONTAINER])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

pub async fn cleanup_cloudflared(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    let cf_dir = base.join("cloudflared");
    if cf_dir.join("docker-compose.yml").exists() {
        if let Err(e) = stop_cloudflared(base).await {
            actions.push(format!("Warning: stop cloudflared: {e}"));
        } else {
            actions.push("Stopped cloudflared".to_string());
        }
    }

    // Force-remove container
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", CLOUDFLARED_CONTAINER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    if cf_dir.exists() {
        let _ = std::fs::remove_dir_all(&cf_dir);
        actions.push("Removed cloudflared directory".to_string());
    }

    actions
}

// ── Domain binding helpers ──────────────────────────────────────────────────

/// Check if any service already uses the given FQDN.
pub fn fqdn_in_use(base: &Path, fqdn: &str, exclude_service: &str) -> bool {
    for id in config::list_installed_services(base) {
        if id == exclude_service {
            continue;
        }
        if let Ok(state) = config::load_service_state(base, &id) {
            if let Some(ref domain) = state.domain {
                if build_fqdn(&domain.subdomain, &domain.zone_name) == fqdn {
                    return true;
                }
            }
        }
    }
    false
}

/// Bind a domain to a service: create CNAME, update ingress, save state.
pub async fn bind_domain(
    base: &Path,
    service_id: &str,
    subdomain: &str,
    zone_id: &str,
    zone_name: &str,
) -> Result<DomainBinding, ServiceError> {
    let cf_config = config::try_load_cloudflare(base);
    let api_token = cf_config
        .api_token
        .as_deref()
        .ok_or_else(|| ServiceError::Io("Cloudflare API token not configured".to_string()))?;
    let account_id = cf_config
        .account_id
        .as_deref()
        .ok_or_else(|| ServiceError::Io("Cloudflare account ID not configured".to_string()))?;
    let tunnel_id = cf_config
        .tunnel_id
        .as_deref()
        .ok_or_else(|| ServiceError::Io("Cloudflare tunnel ID not configured".to_string()))?;

    let fqdn = build_fqdn(subdomain, zone_name);

    // Check for duplicate FQDNs
    if fqdn_in_use(base, &fqdn, service_id) {
        return Err(ServiceError::Io(format!(
            "Domain {fqdn} is already bound to another service"
        )));
    }

    let client = CloudflareClient::new(api_token);

    // If service already has a domain binding, remove old DNS record first
    let mut svc_state = config::load_service_state(base, service_id)?;
    if let Some(ref old_domain) = svc_state.domain {
        if let Some(ref old_record_id) = old_domain.dns_record_id {
            let _ = client
                .delete_dns_record(&old_domain.zone_id, old_record_id)
                .await;
        }
    }

    // Create CNAME
    let record_id = client
        .create_cname(zone_id, &fqdn, tunnel_id, true)
        .await?;

    // Build and save binding
    let binding = DomainBinding {
        subdomain: subdomain.to_string(),
        zone_id: zone_id.to_string(),
        zone_name: zone_name.to_string(),
        dns_record_id: Some(record_id),
    };
    svc_state.domain = Some(binding.clone());
    config::save_service_state(base, service_id, &svc_state)?;

    // Update tunnel ingress
    let rules = collect_ingress_rules(base);
    if let Err(e) = client
        .update_tunnel_ingress(account_id, tunnel_id, rules)
        .await
    {
        tracing::warn!("Failed to update ingress after bind: {e}");
    }

    Ok(binding)
}

/// Unbind a domain from a service: delete DNS record, update ingress, clear state.
pub async fn unbind_domain(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let cf_config = config::try_load_cloudflare(base);
    let api_token = cf_config.api_token.as_deref();
    let account_id = cf_config.account_id.as_deref();
    let tunnel_id = cf_config.tunnel_id.as_deref();

    let mut svc_state = config::load_service_state(base, service_id)?;
    let domain = svc_state
        .domain
        .take()
        .ok_or_else(|| ServiceError::Io("Service has no domain binding".to_string()))?;

    // Delete DNS record (best-effort)
    if let (Some(token), Some(record_id)) = (api_token, &domain.dns_record_id) {
        let client = CloudflareClient::new(token);
        if let Err(e) = client
            .delete_dns_record(&domain.zone_id, record_id)
            .await
        {
            tracing::warn!("Failed to delete DNS record for {service_id}: {e}");
        }
    }

    // Save cleared state
    config::save_service_state(base, service_id, &svc_state)?;

    // Update tunnel ingress
    if let (Some(token), Some(aid), Some(tid)) = (api_token, account_id, tunnel_id) {
        let client = CloudflareClient::new(token);
        let rules = collect_ingress_rules(base);
        if let Err(e) = client.update_tunnel_ingress(aid, tid, rules).await {
            tracing::warn!("Failed to update ingress after unbind: {e}");
        }
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_fqdn_with_subdomain() {
        assert_eq!(build_fqdn("photos", "borodutch.com"), "photos.borodutch.com");
    }

    #[test]
    fn build_fqdn_apex() {
        assert_eq!(build_fqdn("", "borodutch.com"), "borodutch.com");
    }

    #[test]
    fn generate_compose_contains_essentials() {
        let compose = generate_cloudflared_compose("test-token-123");
        assert!(compose.contains("cloudflare/cloudflared:latest"));
        assert!(compose.contains(CLOUDFLARED_CONTAINER));
        assert!(compose.contains("network_mode: host"));
        assert!(compose.contains("test-token-123"));
        assert!(compose.contains("tunnel run"));
    }

    #[test]
    fn collect_ingress_rules_empty_has_catch_all() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let rules = collect_ingress_rules(base);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].hostname.is_none());
        assert_eq!(rules[0].service, "http_status:404");
    }

    #[test]
    fn collect_ingress_rules_includes_bound_services() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::ServiceState {
            installed: true,
            port: Some(9001),
            domain: Some(DomainBinding {
                subdomain: "photos".to_string(),
                zone_id: "z1".to_string(),
                zone_name: "example.com".to_string(),
                dns_record_id: Some("r1".to_string()),
            }),
            ..Default::default()
        };
        config::save_service_state(base, "immich", &state).unwrap();

        let rules = collect_ingress_rules(base);
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0].hostname.as_deref(),
            Some("photos.example.com")
        );
        assert_eq!(rules[0].service, "http://localhost:9001");
        assert!(rules[1].hostname.is_none());
    }

    #[test]
    fn fqdn_in_use_detects_collision() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::ServiceState {
            installed: true,
            port: Some(9001),
            domain: Some(DomainBinding {
                subdomain: "photos".to_string(),
                zone_id: "z1".to_string(),
                zone_name: "example.com".to_string(),
                dns_record_id: None,
            }),
            ..Default::default()
        };
        config::save_service_state(base, "immich", &state).unwrap();

        assert!(fqdn_in_use(base, "photos.example.com", "other-service"));
        assert!(!fqdn_in_use(base, "photos.example.com", "immich")); // Exclude self
        assert!(!fqdn_in_use(base, "files.example.com", "other-service"));
    }

    #[test]
    fn ingress_rule_serialization() {
        let rules = vec![
            IngressRule {
                hostname: Some("test.example.com".to_string()),
                service: "http://localhost:9001".to_string(),
            },
            IngressRule {
                hostname: None,
                service: "http_status:404".to_string(),
            },
        ];

        let json = serde_json::to_value(&rules).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr[0].get("hostname").is_some());
        assert!(arr[1].get("hostname").is_none());
    }
}
