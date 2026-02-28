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

/// List subdirectories at a given path for the file picker.
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
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

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
            // Skip hidden dirs and system pseudo-filesystems
            if name.starts_with('.') {
                continue;
            }
            let entry_path = entry.path().to_string_lossy().to_string();
            entries.push(DirEntry {
                name,
                path: entry_path,
            });
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));

    Json(BrowseResult {
        path: canonical.to_string_lossy().to_string(),
        entries,
    })
}
