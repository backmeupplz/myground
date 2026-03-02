use axum::extract::Query;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Deserialize, IntoParams)]
pub struct BrowseQuery {
    #[serde(default = "default_path")]
    pub path: String,
}

fn default_path() -> String {
    "/".to_string()
}

#[derive(Serialize, ToSchema)]
pub struct BrowseResult {
    pub path: String,
    pub entries: Vec<DirEntry>,
}

#[derive(Serialize, ToSchema)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

/// Paths that should never be browseable.
const BLOCKED_PATHS: &[&str] = &[
    "/proc", "/sys", "/dev", "/run", "/snap", "/boot", "/lost+found",
    "/etc", "/root", "/var/run", "/tmp", "/var/lib/docker",
];

/// Check if a canonicalized path is safe to browse (not a sensitive system directory).
/// Returns false for any path that cannot be canonicalized (fail closed).
fn is_safe_path(path: &std::path::Path) -> bool {
    let Some(canonical) = path.canonicalize().ok() else {
        return false; // fail closed: if we can't resolve the path, deny it
    };
    let s = canonical.to_string_lossy();
    !BLOCKED_PATHS.iter().any(|blocked| s.starts_with(blocked))
}

/// List subdirectories at a given path for the file picker.
/// Only lists directories, skips hidden entries and sensitive system paths.
#[utoipa::path(
    get,
    path = "/browse",
    params(BrowseQuery),
    responses(
        (status = 200, description = "Directory listing", body = BrowseResult)
    )
)]
pub async fn browse(Query(query): Query<BrowseQuery>) -> Json<BrowseResult> {
    let path = std::path::Path::new(&query.path);
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => {
            return Json(BrowseResult {
                path: query.path,
                entries: Vec::new(),
            });
        }
    };

    if !is_safe_path(&canonical) {
        return Json(BrowseResult {
            path: canonical.to_string_lossy().to_string(),
            entries: Vec::new(),
        });
    }

    // Block access to the myground data directory
    let data_dir = crate::config::data_dir();
    if let Ok(dd) = data_dir.canonicalize() {
        if canonical.starts_with(&dd) {
            return Json(BrowseResult {
                path: canonical.to_string_lossy().to_string(),
                entries: Vec::new(),
            });
        }
    }

    let mut entries = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(&canonical) {
        for entry in read_dir.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden dirs
            if name.starts_with('.') {
                continue;
            }
            let entry_path = entry.path();
            // Skip sensitive child directories
            if !is_safe_path(&entry_path) {
                continue;
            }
            entries.push(DirEntry {
                name,
                path: entry_path.to_string_lossy().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));

    Json(BrowseResult {
        path: canonical.to_string_lossy().to_string(),
        entries,
    })
}
