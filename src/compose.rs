use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use crate::config;
use crate::error::AppError;
use crate::registry::AppDefinition;

const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_TIMEOUT: Duration = Duration::from_secs(300);

/// Detect whether `docker compose` (v2) or `docker-compose` (v1) is available.
pub async fn detect_command() -> Result<Vec<String>, AppError> {
    // Try v2 first
    let v2 = tokio::time::timeout(
        DETECT_TIMEOUT,
        tokio::process::Command::new("docker")
            .args(["compose", "version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;

    if let Ok(Ok(status)) = v2 {
        if status.success() {
            return Ok(vec!["docker".to_string(), "compose".to_string()]);
        }
    }

    // Try v1
    let v1 = tokio::time::timeout(
        DETECT_TIMEOUT,
        tokio::process::Command::new("docker-compose")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;

    if let Ok(Ok(status)) = v1 {
        if status.success() {
            return Ok(vec!["docker-compose".to_string()]);
        }
    }

    Err(AppError::Compose(
        "Neither 'docker compose' nor 'docker-compose' found".to_string(),
    ))
}

/// Generate docker-compose.yml content from an app definition and env vars.
///
/// Each `${KEY}` slot gets its value escaped for the YAML scalar it sits in
/// (double-quoted, single-quoted or plain). The parser decides which escaping
/// keeps the value intact; a value no escaping can hold there is refused.
/// Without `escaped` (installs made before this), values go in raw as long as
/// the result parses, exactly as they always did.
pub fn generate_compose(
    def: &AppDefinition,
    env: &HashMap<String, String>,
    escaped: bool,
) -> Result<String, AppError> {
    // Split the template into literal text and `${KEY}` slots: lits.len() == slots.len() + 1.
    let tpl = def.compose_template.as_str();
    let (mut lits, mut slots) = (Vec::new(), Vec::new());
    let (mut start, mut pos) = (0, 0);
    while let Some(off) = tpl[pos..].find("${") {
        let open = pos + off;
        let Some(len) = tpl[open + 2..].find('}') else {
            break;
        };
        let key = &tpl[open + 2..open + 2 + len];
        pos = open + 2 + len + 1;
        if let Some(value) = env.get(key) {
            lits.push(&tpl[start..open]);
            // Values are literals: escape `$` so compose doesn't interpolate them.
            slots.push((key, value.replace('$', "$$")));
            start = pos;
        } else {
            pos = open + 2;
        }
    }
    lits.push(&tpl[start..]);

    let render = |slot: &dyn Fn(usize) -> String| {
        let mut out = lits[0].to_string();
        for (i, lit) in lits[1..].iter().enumerate() {
            out.push_str(&slot(i));
            out.push_str(lit);
        }
        out
    };
    let parse = |s: &str| serde_yaml::from_str::<serde_yaml::Value>(s).ok();
    // Terminated, so `mgslot1z` is not a prefix of `mgslot10z`.
    let token = |j: usize| format!("mgslot{j}z");

    if !escaped {
        let raw = render(&|i| slots[i].1.clone());
        if parse(&raw).is_some() {
            return Ok(raw);
        }
    }

    let mut chosen: Vec<String> = Vec::with_capacity(slots.len());
    for (i, (key, value)) in slots.iter().enumerate() {
        // Values made only of these are written as is, checked only below.
        if plain_safe(value) {
            chosen.push(value.clone());
            continue;
        }
        let with = |v: &str| {
            render(&|j| {
                if j == i { v.to_string() } else { token(j) }
            })
        };
        // A broken template is left for validate_compose to report.
        let Some(mut expected) = parse(&with("mgtoken")) else {
            chosen.push(value.clone());
            continue;
        };
        swap_token(&mut expected, "mgtoken", value);
        let dq = value.replace('\\', "\\\\").replace('"', "\\\"");
        let candidates = [
            dq.clone(),                // inside "…"
            value.replace('\'', "''"), // inside '…'
            value.clone(),             // plain
            // A whole plain slot (`KEY: ${V}`) quoted. Only reached when plain
            // can't hold the value, so plain values keep their YAML types.
            format!("\"{dq}\""),
        ];
        let Some(fit) = candidates
            .into_iter()
            .find(|c| parse(&with(c)).as_ref() == Some(&expected))
        else {
            return Err(AppError::Invalid(format!(
                "The value of {key} has characters this app's compose file can't hold"
            )));
        };
        chosen.push(fit);
    }

    // Each slot was checked alone; check them together too: `${A}${B}` with
    // A=`a ` and B=`#b` renders `a #b`, which YAML reads as `a`. Slots 0..n
    // hold their values, the rest their tokens.
    if let Some(skeleton) = parse(&render(&|j| token(j))) {
        let holds = |n: usize| {
            let mut want = skeleton.clone();
            for (j, (_, value)) in slots[..n].iter().enumerate() {
                swap_token(&mut want, &token(j), value);
            }
            let got = parse(&render(&|j| {
                if j < n { chosen[j].clone() } else { token(j) }
            }));
            got.is_some_and(|got| same(&got, &want))
        };
        if !holds(slots.len()) {
            let bad = (1..=slots.len())
                .find(|&n| !holds(n))
                .unwrap_or(slots.len());
            return Err(AppError::Invalid(format!(
                "The value of {} has characters this app's compose file can't hold",
                slots[bad - 1].0
            )));
        }
    }
    Ok(render(&|i| chosen[i].clone()))
}

/// Values made only of these characters need no escaping in any slot.
fn plain_safe(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_.-/+=~$".contains(c))
}

/// Equal, except that a plain-safe string may come back as whatever YAML types
/// its text as (`123`, `1.50`, `true`, `~`): compose always read such values so.
fn same(got: &serde_yaml::Value, want: &serde_yaml::Value) -> bool {
    use serde_yaml::Value;
    match (got, want) {
        (Value::Sequence(a), Value::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        }
        (Value::Mapping(a), Value::Mapping(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((ka, va), (kb, vb))| same(ka, kb) && same(va, vb))
        }
        (Value::Tagged(a), Value::Tagged(b)) => a.tag == b.tag && same(&a.value, &b.value),
        (Value::Null | Value::Bool(_) | Value::Number(_), Value::String(s)) => {
            !s.is_empty()
                && plain_safe(s)
                && serde_yaml::from_str::<Value>(s).ok().as_ref() == Some(got)
        }
        _ => got == want,
    }
}

/// Replace `token` with `with` inside every string (keys included).
fn swap_token(v: &mut serde_yaml::Value, token: &str, with: &str) {
    use serde_yaml::Value;
    match v {
        Value::String(s) => *s = s.replace(token, with),
        Value::Sequence(seq) => seq.iter_mut().for_each(|x| swap_token(x, token, with)),
        Value::Mapping(m) => {
            *m = std::mem::take(m)
                .into_iter()
                .map(|(mut k, mut x)| {
                    swap_token(&mut k, token, with);
                    swap_token(&mut x, token, with);
                    (k, x)
                })
                .collect();
        }
        Value::Tagged(t) => swap_token(&mut t.value, token, with),
        _ => {}
    }
}

/// Generate .env file content from defaults merged with overrides.
pub fn generate_env_file(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> String {
    let merged = merge_env(defaults, overrides);
    let mut lines: Vec<String> = merged
        .iter()
        .map(|(k, v)| {
            if !v.contains(['$', '"', '\'', '\\', '#', ' ', '\t']) {
                format!("{k}={v}")
            } else if !v.contains(['\'', '\\']) {
                // Single quotes keep compose from interpolating `$` (e.g. Argon2
                // hashes). Not for `\`: compose reads `\'` as an escaped quote.
                format!("{k}='{v}'")
            } else {
                let v = v
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "\\$");
                format!("{k}=\"{v}\"")
            }
        })
        .collect();
    lines.sort();
    lines.join("\n") + "\n"
}

/// Merge defaults with user overrides.
pub fn merge_env(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = defaults.clone();
    for (k, v) in overrides {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Validate that a composed YAML string is structurally valid.
pub fn validate_compose(yaml: &str) -> Result<(), AppError> {
    let _: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| AppError::Io(format!("Invalid compose YAML after substitution: {e}")))?;
    Ok(())
}

/// Restrict file permissions to owner-only (0o600).
pub fn restrict_file_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Argon2id PHC hash with Bitwarden's preset (what `vaultwarden hash` produces).
/// Already-hashed values pass through unchanged.
pub fn argon2_hash(value: &str) -> Result<String, AppError> {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    if value.starts_with("$argon2") {
        return Ok(value.to_string());
    }
    let params = argon2::Params::new(65540, 3, 4, None)
        .map_err(|e| AppError::Io(format!("Argon2 params: {e}")))?;
    let hasher = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let salt = SaltString::generate(&mut OsRng);
    hasher
        .hash_password(value.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Io(format!("Argon2 hash: {e}")))
}

/// Validate that an env value does not contain newlines or other control characters
/// that could break .env files or YAML templates.
pub fn validate_env_value(value: &str) -> Result<(), AppError> {
    if value.contains('\n') || value.contains('\r') {
        return Err(AppError::Invalid(
            "Env value must not contain newline characters".to_string(),
        ));
    }
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(AppError::Invalid(
            "Env value must not contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate that an env key contains only `[A-Z0-9_]` characters.
pub fn validate_env_key(key: &str) -> Result<(), AppError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::Invalid(format!(
            "Invalid env key '{key}': must contain only A-Z, 0-9, _"
        )));
    }
    Ok(())
}

/// Run a docker compose command in an app directory.
pub async fn run(
    compose_cmd: &[String],
    work_dir: &Path,
    args: &[&str],
) -> Result<String, AppError> {
    let (program, base_args) = compose_cmd.split_first().expect("compose_cmd is non-empty");

    let output = tokio::time::timeout(
        RUN_TIMEOUT,
        tokio::process::Command::new(program)
            .args(base_args)
            .args(args)
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| AppError::Compose("Compose command timed out after 300s".to_string()))?
    .map_err(|e| AppError::Compose(format!("Failed to run compose: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Compose(format!(
            "Compose command failed: {stderr}"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a docker compose command, streaming stdout+stderr lines via a channel.
pub async fn run_streaming(
    compose_cmd: &[String],
    work_dir: &Path,
    args: &[&str],
    tx: &tokio::sync::mpsc::Sender<String>,
) -> Result<(), AppError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let (program, base_args) = compose_cmd.split_first().expect("compose_cmd is non-empty");

    let mut child = tokio::process::Command::new(program)
        .args(base_args)
        .args(args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Compose(format!("Failed to run compose: {e}")))?;

    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let tx2 = tx.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx2.send(line).await.is_err() {
                break;
            }
        }
    });

    let tx3 = tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx3.send(line).await.is_err() {
                break;
            }
        }
    });

    let status = tokio::time::timeout(RUN_TIMEOUT, child.wait())
        .await
        .map_err(|_| AppError::Compose("Compose command timed out after 300s".to_string()))?
        .map_err(|e| AppError::Compose(format!("compose wait: {e}")))?;

    let _ = stderr_task.await;
    let _ = stdout_task.await;

    if !status.success() {
        return Err(AppError::Compose("Compose command failed".to_string()));
    }

    Ok(())
}

/// Deploy (pull + start) an app without streaming output.
pub async fn deploy(base: &Path, app_id: &str) -> Result<(), AppError> {
    let svc_dir = config::app_dir(base, app_id);
    let compose_cmd = detect_command().await?;

    run(&compose_cmd, &svc_dir, &["pull"]).await?;
    // --force-recreate ensures stale containers from previous installs are replaced
    run(
        &compose_cmd,
        &svc_dir,
        &["up", "-d", "--force-recreate", "--remove-orphans"],
    )
    .await?;

    // Record the image digest for update tracking
    let compose_path = svc_dir.join("docker-compose.yml");
    if compose_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&compose_path) {
            if let Some(image_ref) = crate::updates::extract_primary_image(&content) {
                if let Ok(digest) = crate::updates::get_image_digest(&image_ref).await {
                    if let Ok(mut svc_state) = config::load_app_state(base, app_id) {
                        svc_state.image_digest = Some(digest);
                        svc_state.latest_image_digest = None;
                        svc_state.update_available = false;
                        let _ = config::save_app_state(base, app_id, &svc_state);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Deploy (pull + start) an app, streaming progress lines via a channel.
/// After a successful deploy, records the primary image digest in InstalledAppState.
pub async fn deploy_streaming(
    base: &Path,
    app_id: &str,
    tx: tokio::sync::mpsc::Sender<String>,
) -> Result<(), AppError> {
    let svc_dir = config::app_dir(base, app_id);
    let compose_cmd = detect_command().await?;

    let _ = tx.send("Pulling images...".to_string()).await;
    run_streaming(&compose_cmd, &svc_dir, &["pull"], &tx).await?;

    let _ = tx.send("Starting containers...".to_string()).await;
    // --force-recreate ensures stale containers from previous installs are replaced
    run_streaming(
        &compose_cmd,
        &svc_dir,
        &["up", "-d", "--force-recreate", "--remove-orphans"],
        &tx,
    )
    .await?;

    // Record the image digest for update tracking
    let compose_path = svc_dir.join("docker-compose.yml");
    if compose_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&compose_path) {
            if let Some(image_ref) = crate::updates::extract_primary_image(&content) {
                if let Ok(digest) = crate::updates::get_image_digest(&image_ref).await {
                    if let Ok(mut svc_state) = config::load_app_state(base, app_id) {
                        svc_state.image_digest = Some(digest);
                        svc_state.latest_image_digest = None;
                        svc_state.update_available = false;
                        let _ = config::save_app_state(base, app_id, &svc_state);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_hash_survives_compose_and_env() {
        use argon2::PasswordVerifier;
        let h = argon2_hash("s3cret").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=65540,t=3,p=4$"));
        let parsed = argon2::PasswordHash::new(&h).unwrap();
        assert!(
            argon2::Argon2::default()
                .verify_password(b"s3cret", &parsed)
                .is_ok()
        );
        assert_eq!(argon2_hash(&h).unwrap(), h);

        let def = dummy_app_def("vw", "x: \"${ADMIN_TOKEN}\"", HashMap::new(), vec![]);
        let env = HashMap::from([("ADMIN_TOKEN".to_string(), h.clone())]);
        assert_eq!(
            generate_compose(&def, &env, true).unwrap(),
            format!("x: \"{}\"", h.replace('$', "$$"))
        );
        assert!(generate_env_file(&HashMap::new(), &env).contains(&format!("ADMIN_TOKEN='{h}'")));
    }
    use crate::testutil::{dummy_app_def, dummy_storage_volumes};

    #[test]
    fn generate_compose_substitutes_vars() {
        let def = dummy_app_def(
            "test",
            "ports:\n  - \"${PORT}:80\"",
            HashMap::from([("PORT".to_string(), "8080".to_string())]),
            Vec::new(),
        );

        let env = HashMap::from([("PORT".to_string(), "9090".to_string())]);
        let result = generate_compose(&def, &env, true).unwrap();
        assert_eq!(result, "ports:\n  - \"9090:80\"");
    }

    #[test]
    fn generate_env_file_merges_correctly() {
        let defaults = HashMap::from([
            ("PORT".to_string(), "8080".to_string()),
            ("HOST".to_string(), "localhost".to_string()),
        ]);
        let overrides = HashMap::from([("PORT".to_string(), "9090".to_string())]);

        let result = generate_env_file(&defaults, &overrides);
        assert!(result.contains("PORT=9090"));
        assert!(result.contains("HOST=localhost"));
        assert!(!result.contains("PORT=8080"));
    }

    #[test]
    fn merge_env_applies_overrides() {
        let defaults = HashMap::from([("A".to_string(), "1".to_string())]);
        let overrides = HashMap::from([
            ("A".to_string(), "2".to_string()),
            ("B".to_string(), "3".to_string()),
        ]);
        let merged = merge_env(&defaults, &overrides);
        assert_eq!(merged.get("A").unwrap(), "2");
        assert_eq!(merged.get("B").unwrap(), "3");
    }

    #[test]
    fn generate_compose_with_storage_vars() {
        let def = dummy_app_def(
            "fb",
            "volumes:\n  - ${STORAGE_data}:/srv\n  - ${STORAGE_config}:/config",
            HashMap::new(),
            dummy_storage_volumes(),
        );

        let env = HashMap::from([
            ("STORAGE_data".to_string(), "/mnt/data/fb/data".to_string()),
            (
                "STORAGE_config".to_string(),
                "/mnt/data/fb/config".to_string(),
            ),
        ]);

        let result = generate_compose(&def, &env, true).unwrap();
        assert!(result.contains("/mnt/data/fb/data:/srv"));
        assert!(result.contains("/mnt/data/fb/config:/config"));
        assert!(!result.contains("${STORAGE_"));
    }

    #[test]
    fn validate_env_value_accepts_normal() {
        assert!(validate_env_value("hello world").is_ok());
        assert!(validate_env_value("p@ssw0rd!#$%^&*").is_ok());
        assert!(validate_env_value("").is_ok());
        assert!(validate_env_value("with\ttab").is_ok());
    }

    #[test]
    fn validate_env_value_rejects_newlines() {
        assert!(validate_env_value("line1\nline2").is_err());
        assert!(validate_env_value("line1\r\nline2").is_err());
        assert!(validate_env_value("line1\rline2").is_err());
    }

    #[test]
    fn validate_env_value_rejects_control_chars() {
        assert!(validate_env_value("null\x00byte").is_err());
        assert!(validate_env_value("bell\x07char").is_err());
    }

    #[test]
    fn validate_compose_accepts_valid_yaml() {
        assert!(validate_compose("services:\n  app:\n    image: nginx\n").is_ok());
    }

    #[test]
    fn validate_compose_rejects_invalid_yaml() {
        assert!(validate_compose("{{invalid: yaml: [").is_err());
    }

    #[test]
    fn validate_env_key_accepts_valid() {
        assert!(validate_env_key("PORT").is_ok());
        assert!(validate_env_key("MY_VAR_123").is_ok());
        assert!(validate_env_key("A").is_ok());
    }

    #[test]
    fn validate_env_key_rejects_empty() {
        assert!(validate_env_key("").is_err());
    }

    #[test]
    fn validate_env_key_rejects_lowercase() {
        assert!(validate_env_key("port").is_err());
        assert!(validate_env_key("myVar").is_err());
    }

    #[test]
    fn validate_env_key_rejects_special_chars() {
        assert!(validate_env_key("MY-VAR").is_err());
        assert!(validate_env_key("MY.VAR").is_err());
        assert!(validate_env_key("MY VAR").is_err());
    }

    #[test]
    fn restrict_file_permissions_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "test").unwrap();
        restrict_file_permissions(&path);
        // File should still be readable by owner
        assert!(std::fs::read_to_string(&path).is_ok());
    }

    #[test]
    fn generate_env_file_sorted_output() {
        let defaults = HashMap::from([
            ("ZEBRA".to_string(), "1".to_string()),
            ("ALPHA".to_string(), "2".to_string()),
        ]);
        let result = generate_env_file(&defaults, &HashMap::new());
        let lines: Vec<&str> = result.trim().split('\n').collect();
        assert_eq!(lines[0], "ALPHA=2");
        assert_eq!(lines[1], "ZEBRA=1");
    }

    #[test]
    fn generate_compose_keeps_quotes_and_backslashes_intact() {
        let tpl = "services:\n  app:\n    image: x\n    environment:\n      DQ: \"${V}\"\n      SQ: '${V}'\n      PLAIN: x ${V}\n    command: [\"--pw\", \"${V}\", \"end\"]\n    healthcheck:\n      test: cmd --eval \"x '${V}'\"\n";
        let def = dummy_app_def("app", tpl, HashMap::new(), Vec::new());
        for v in [r#"p"a\ss'w$rd"#, r#"x", "y"#, r#"\"#, "it's", r#"a\nb"#] {
            let env = HashMap::from([("V".to_string(), v.to_string())]);
            let yaml = generate_compose(&def, &env, true).unwrap();
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
            let app = &doc["services"]["app"];
            let want = v.replace('$', "$$");
            assert_eq!(
                app["environment"]["DQ"].as_str(),
                Some(want.as_str()),
                "{yaml}"
            );
            assert_eq!(
                app["environment"]["SQ"].as_str(),
                Some(want.as_str()),
                "{yaml}"
            );
            let plain = format!("x {want}");
            assert_eq!(app["environment"]["PLAIN"].as_str(), Some(plain.as_str()));
            assert_eq!(app["command"].as_sequence().unwrap().len(), 3, "{yaml}");
            assert_eq!(app["command"][1].as_str(), Some(want.as_str()));
            let test = format!("cmd --eval \"x '{want}'\"");
            assert_eq!(app["healthcheck"]["test"].as_str(), Some(test.as_str()));
        }
    }

    #[test]
    fn generate_compose_refuses_what_a_slot_cannot_hold() {
        let def = dummy_app_def("a", "x: z ${V}\ny: \"${W}\"\n", HashMap::new(), Vec::new());
        for bad in ["a #b", "a: b"] {
            let env = HashMap::from([
                ("V".to_string(), bad.to_string()),
                ("W".to_string(), "w".to_string()),
            ]);
            let err = generate_compose(&def, &env, true).unwrap_err();
            assert!(matches!(err, AppError::Invalid(_)), "{bad}: {err}");
            assert!(err.to_string().contains('V'), "{err}");
        }
        // The same values fit a double-quoted slot.
        for ok in ["a #b", "a: b", "\"a"] {
            let env = HashMap::from([
                ("V".to_string(), "v".to_string()),
                ("W".to_string(), ok.to_string()),
            ]);
            let doc: serde_yaml::Value =
                serde_yaml::from_str(&generate_compose(&def, &env, true).unwrap()).unwrap();
            assert_eq!(doc["y"].as_str(), Some(ok));
        }
    }

    #[test]
    fn generate_env_file_quotes_values_compose_would_misread() {
        let env = HashMap::from([
            ("A".to_string(), "\"abc".to_string()),
            ("B".to_string(), "a #b".to_string()),
            ("C".to_string(), r#"p"a'\s$s"#.to_string()),
            ("D".to_string(), "plain-1.2".to_string()),
            ("E".to_string(), "abc\\".to_string()),
        ]);
        let out = generate_env_file(&HashMap::new(), &env);
        assert!(out.contains("A='\"abc'\n"), "{out}");
        assert!(out.contains("B='a #b'\n"), "{out}");
        assert!(out.contains(r#"C="p\"a'\\s\$s""#), "{out}");
        assert!(out.contains("D=plain-1.2\n"), "{out}");
        assert!(out.contains("E=\"abc\\\\\"\n"), "{out}");
    }

    #[test]
    fn generate_compose_checks_slots_together() {
        let def = dummy_app_def("a", "x: ${A}${B}\n", HashMap::new(), Vec::new());
        let env = HashMap::from([
            ("A".to_string(), "a ".to_string()),
            ("B".to_string(), "#b".to_string()),
        ]);
        let err = generate_compose(&def, &env, true).unwrap_err();
        assert!(matches!(err, AppError::Invalid(_)), "{err}");
        assert!(err.to_string().contains(" B "), "{err}");

        let def = dummy_app_def("a", "x: ${A} ${B}\n", HashMap::new(), Vec::new());
        let env = HashMap::from([
            ("A".to_string(), String::new()),
            ("B".to_string(), "&b".to_string()),
        ]);
        assert!(matches!(
            generate_compose(&def, &env, true),
            Err(AppError::Invalid(_))
        ));

        // An empty value in a whole plain slot would turn into null.
        let def = dummy_app_def("a", "x: ${A}\n", HashMap::new(), Vec::new());
        let env = HashMap::from([("A".to_string(), String::new())]);
        assert!(matches!(
            generate_compose(&def, &env, true),
            Err(AppError::Invalid(_))
        ));

        // 12 slots: token 1 must not match inside tokens 10 and 11.
        let tpl: String = (0..12).map(|i| format!("${{V{i}}}-")).collect();
        let def = dummy_app_def("a", &format!("x: {tpl}\n"), HashMap::new(), Vec::new());
        let env: HashMap<_, _> = (0..12)
            .map(|i| (format!("V{i}"), format!("v{i}")))
            .collect();
        let want: String = (0..12).map(|i| format!("v{i}-")).collect();
        assert_eq!(
            generate_compose(&def, &env, true).unwrap(),
            format!("x: {want}\n")
        );
    }

    #[test]
    fn generate_compose_keeps_plain_slots_yaml_types_as_before() {
        let def = dummy_app_def("a", "x: ${V}\n", HashMap::new(), Vec::new());
        for v in [
            "01234567", "1.50", "1e3", "0x1F", "123", "true", "~", "null",
        ] {
            let env = HashMap::from([("V".to_string(), v.to_string())]);
            assert_eq!(
                generate_compose(&def, &env, true).unwrap(),
                format!("x: {v}\n")
            );
        }
        // YAML would drop these quotes, or read the rest as a tag, anchor,
        // alias, comment or mapping, so the whole slot is quoted.
        for v in [
            "'secret'",
            "!Secret1",
            "@dmin2024",
            "*a",
            "&a",
            "a #b",
            "a: b",
            " a ",
        ] {
            let env = HashMap::from([("V".to_string(), v.to_string())]);
            let yaml = generate_compose(&def, &env, true).unwrap();
            assert!(yaml.starts_with("x: \""), "{yaml}");
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(doc["x"].as_str(), Some(v), "{yaml}");
        }
        // Values plain holds stay unquoted, as before.
        for v in ["a, b", "p@ss word", "a\tb"] {
            let env = HashMap::from([("V".to_string(), v.to_string())]);
            assert_eq!(
                generate_compose(&def, &env, true).unwrap(),
                format!("x: {v}\n")
            );
        }
        // YAML folds a line break even inside quotes.
        let env = HashMap::from([("V".to_string(), "a\nb".to_string())]);
        assert!(matches!(
            generate_compose(&def, &env, true),
            Err(AppError::Invalid(_))
        ));
        // Quoting only helps where the slot is the whole value.
        let def = dummy_app_def("a", "x: z${V}\n", HashMap::new(), Vec::new());
        let env = HashMap::from([("V".to_string(), " #b".to_string())]);
        assert!(matches!(
            generate_compose(&def, &env, true),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn immich_takes_db_passwords_yaml_would_misread() {
        let def = &crate::registry::load_registry()["immich"];
        let env = HashMap::from([("IMMICH_DB_PASSWORD".to_string(), "!Secret1".to_string())]);
        let yaml = generate_compose(def, &env, true).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let s = &doc["services"];
        assert_eq!(
            s["immich-server"]["environment"]["DB_PASSWORD"].as_str(),
            Some("!Secret1")
        );
        assert_eq!(
            s["immich-db"]["environment"]["POSTGRES_PASSWORD"].as_str(),
            Some("!Secret1")
        );
    }

    #[test]
    fn generate_compose_renders_older_installs_raw() {
        let def = dummy_app_def("a", "x: \"${A}\"\ny: ${B}\n", HashMap::new(), Vec::new());
        let env = HashMap::from([
            ("A".to_string(), r"C:\temp".to_string()),
            ("B".to_string(), "pa ss #1".to_string()),
        ]);
        assert_eq!(
            generate_compose(&def, &env, false).unwrap(),
            "x: \"C:\\temp\"\ny: pa ss #1\n"
        );
        assert_eq!(
            generate_compose(&def, &env, true).unwrap(),
            "x: \"C:\\\\temp\"\ny: \"pa ss #1\"\n"
        );
        // Raw that doesn't parse is escaped, as for new installs.
        let env = HashMap::from([
            ("A".to_string(), "a\"b".to_string()),
            ("B".to_string(), "b".to_string()),
        ]);
        assert_eq!(
            generate_compose(&def, &env, false).unwrap(),
            "x: \"a\\\"b\"\ny: b\n"
        );
    }

    #[test]
    fn bundled_apps_render_with_their_defaults() {
        for (id, def) in crate::registry::load_registry() {
            let mut env = def.defaults.clone();
            let mut rest = def.compose_template.as_str();
            while let Some(open) = rest.find("${") {
                rest = &rest[open + 2..];
                let key = &rest[..rest.find('}').unwrap()];
                env.entry(key.to_string())
                    .or_insert_with(|| "/x/1".to_string());
            }
            let yaml = generate_compose(&def, &env, true).unwrap_or_else(|e| panic!("{id}: {e}"));
            validate_compose(&yaml).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }
}
