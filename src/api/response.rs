use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ActionResponse {
    pub ok: bool,
    pub message: String,
}

pub fn action_ok(msg: impl Into<String>) -> Json<ActionResponse> {
    Json(ActionResponse {
        ok: true,
        message: msg.into(),
    })
}

pub fn action_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ActionResponse>) {
    (
        status,
        Json(ActionResponse {
            ok: false,
            message: sanitize_error(msg.into()),
        }),
    )
}

/// Strip absolute filesystem paths from error messages to avoid leaking
/// internal directory structure to API consumers.
fn sanitize_error(msg: String) -> String {
    let chars: Vec<char> = msg.chars().collect();
    let mut out = String::with_capacity(msg.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric()
                    || matches!(chars[i], '/' | '.' | '-' | '_'))
            {
                i += 1;
            }
            let path: String = chars[start..i].iter().collect();
            if path.matches('/').count() >= 2 {
                // Multi-segment path — replace with basename only
                let basename = path.rsplit('/').next().unwrap_or("...");
                out.push_str(basename);
            } else {
                // Single-segment (e.g., /tmp) — keep as-is
                out.push_str(&path);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_multi_segment_paths() {
        let msg = "Failed to read /home/user/.myground/config.toml: not found".to_string();
        assert_eq!(
            sanitize_error(msg),
            "Failed to read config.toml: not found"
        );
    }

    #[test]
    fn preserves_single_segment_paths() {
        let msg = "Cannot write to /tmp".to_string();
        assert_eq!(sanitize_error(msg), "Cannot write to /tmp");
    }

    #[test]
    fn strips_multiple_paths() {
        let msg = "Copy /home/a/b/src to /var/lib/dest failed".to_string();
        assert_eq!(sanitize_error(msg), "Copy src to dest failed");
    }

    #[test]
    fn preserves_no_path_messages() {
        let msg = "Something went wrong".to_string();
        assert_eq!(sanitize_error(msg), "Something went wrong");
    }

    #[test]
    fn handles_path_at_end() {
        let msg = "Error at /home/user/.myground/apps/foo".to_string();
        assert_eq!(sanitize_error(msg), "Error at foo");
    }
}
