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
            message: msg.into(),
        }),
    )
}
