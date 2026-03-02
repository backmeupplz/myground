pub mod api;
pub mod auth;
pub mod backup;
pub mod cloudflare;
pub mod compose;
pub mod config;
pub mod disk;
pub mod docker;
pub mod error;
pub mod registry;
pub mod scheduler;
pub mod services;
pub mod state;
pub mod stats;
pub mod tailscale;
pub mod updates;
mod web;

#[cfg(test)]
pub(crate) mod testutil;

pub use api::{build_router, serve};
pub use state::AppState;
