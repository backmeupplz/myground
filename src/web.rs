use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref()),
                (header::CACHE_CONTROL, cache),
            ],
            file.data,
        )
            .into_response();
    }

    // SPA fallback: serve index.html for non-file paths
    if let Some(file) = Assets::get("index.html") {
        return (
            [(header::CACHE_CONTROL, "no-cache")],
            Html(std::str::from_utf8(&file.data).unwrap_or_default().to_string()),
        )
            .into_response();
    }

    (StatusCode::NOT_FOUND, "not found").into_response()
}
