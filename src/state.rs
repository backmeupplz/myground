use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bollard::Docker;
use tokio::sync::Mutex;

use crate::registry::AppDefinition;

/// Tracks failed login attempts for rate limiting.
#[derive(Clone, Default)]
pub struct LoginAttempts {
    /// Map from username -> (fail_count, window_start)
    entries: HashMap<String, (u32, Instant)>,
}

impl LoginAttempts {
    const MAX_ATTEMPTS: u32 = 5;
    const WINDOW_SECS: u64 = 300; // 5 minutes

    /// Returns true if the username is currently rate-limited.
    pub fn is_blocked(&self, username: &str) -> bool {
        if let Some((count, start)) = self.entries.get(username) {
            if start.elapsed().as_secs() < Self::WINDOW_SECS {
                return *count >= Self::MAX_ATTEMPTS;
            }
        }
        false
    }

    /// Record a failed login attempt. Returns true if now blocked.
    pub fn record_failure(&mut self, username: &str) -> bool {
        let entry = self.entries.entry(username.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed().as_secs() >= Self::WINDOW_SECS {
            // Reset window
            *entry = (1, Instant::now());
        } else {
            entry.0 += 1;
        }
        entry.0 >= Self::MAX_ATTEMPTS
    }

    /// Clear attempts for a username (on successful login).
    pub fn clear(&mut self, username: &str) {
        self.entries.remove(username);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub docker: Option<Docker>,
    pub registry: Arc<HashMap<String, AppDefinition>>,
    pub data_dir: PathBuf,
    pub sessions: Arc<RwLock<HashSet<String>>>,
    pub login_attempts: Arc<RwLock<LoginAttempts>>,
    /// Tailscale auth key cached in memory (never persisted to disk).
    /// Used to authenticate new sidecar containers when apps are installed.
    pub tailscale_key: Arc<RwLock<Option<String>>>,
    /// Guard to prevent concurrent setup requests.
    pub setup_lock: Arc<Mutex<()>>,
    /// Per-app WebSocket connection counters.
    pub ws_connections: Arc<RwLock<HashMap<String, Arc<AtomicUsize>>>>,
    /// App IDs currently being deployed (pull + up).
    pub deploying: Arc<RwLock<HashSet<String>>>,
}

const MAX_WS_PER_APP: usize = 5;

/// RAII guard that decrements the WebSocket connection count on drop.
pub struct WsGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for WsGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

impl AppState {
    /// Try to acquire a WebSocket slot for the given app ID.
    /// Returns a guard that releases the slot on drop, or None if at limit.
    pub fn try_ws_slot(&self, app_id: &str) -> Option<WsGuard> {
        let counter = {
            let map = self.ws_connections.read().unwrap();
            map.get(app_id).cloned()
        }
        .unwrap_or_else(|| {
            let mut map = self.ws_connections.write().unwrap();
            map.entry(app_id.to_string())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone()
        });
        let prev = counter.fetch_add(1, Ordering::Relaxed);
        if prev >= MAX_WS_PER_APP {
            counter.fetch_sub(1, Ordering::Relaxed);
            return None;
        }
        Some(WsGuard { counter })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_attempts_not_blocked_initially() {
        let attempts = LoginAttempts::default();
        assert!(!attempts.is_blocked("admin"));
    }

    #[test]
    fn login_attempts_below_threshold() {
        let mut attempts = LoginAttempts::default();
        for _ in 0..4 {
            assert!(!attempts.record_failure("admin"));
        }
        assert!(!attempts.is_blocked("admin"));
    }

    #[test]
    fn login_attempts_blocked_at_threshold() {
        let mut attempts = LoginAttempts::default();
        for _ in 0..4 {
            attempts.record_failure("admin");
        }
        // 5th attempt should trigger block
        assert!(attempts.record_failure("admin"));
        assert!(attempts.is_blocked("admin"));
    }

    #[test]
    fn login_attempts_clear_removes_block() {
        let mut attempts = LoginAttempts::default();
        for _ in 0..5 {
            attempts.record_failure("admin");
        }
        assert!(attempts.is_blocked("admin"));
        attempts.clear("admin");
        assert!(!attempts.is_blocked("admin"));
    }

    #[test]
    fn login_attempts_independent_per_user() {
        let mut attempts = LoginAttempts::default();
        for _ in 0..5 {
            attempts.record_failure("user1");
        }
        assert!(attempts.is_blocked("user1"));
        assert!(!attempts.is_blocked("user2"));
    }

    #[test]
    fn login_attempts_record_returns_blocked_status() {
        let mut attempts = LoginAttempts::default();
        assert!(!attempts.record_failure("x"));
        assert!(!attempts.record_failure("x"));
        assert!(!attempts.record_failure("x"));
        assert!(!attempts.record_failure("x"));
        assert!(attempts.record_failure("x")); // 5th = blocked
        assert!(attempts.record_failure("x")); // still blocked
    }

    #[test]
    fn ws_slot_under_limit() {
        let state = AppState::with_docker(None, PathBuf::from("/tmp/test-ws"));
        let guard = state.try_ws_slot("svc1");
        assert!(guard.is_some());
    }

    #[test]
    fn ws_slot_at_limit_returns_none() {
        let state = AppState::with_docker(None, PathBuf::from("/tmp/test-ws2"));
        let mut guards = Vec::new();
        for _ in 0..5 {
            guards.push(state.try_ws_slot("svc1").unwrap());
        }
        // 6th should fail
        assert!(state.try_ws_slot("svc1").is_none());
    }

    #[test]
    fn ws_slot_guard_drop_frees_slot() {
        let state = AppState::with_docker(None, PathBuf::from("/tmp/test-ws3"));
        let mut guards = Vec::new();
        for _ in 0..5 {
            guards.push(state.try_ws_slot("svc1").unwrap());
        }
        assert!(state.try_ws_slot("svc1").is_none());
        // Drop one guard
        guards.pop();
        // Now a slot should be available
        assert!(state.try_ws_slot("svc1").is_some());
    }

    #[test]
    fn ws_slots_independent_per_app_id() {
        let state = AppState::with_docker(None, PathBuf::from("/tmp/test-ws4"));
        let mut guards = Vec::new();
        for _ in 0..5 {
            guards.push(state.try_ws_slot("svc1").unwrap());
        }
        // svc1 is full, but svc2 should be fine
        assert!(state.try_ws_slot("svc1").is_none());
        assert!(state.try_ws_slot("svc2").is_some());
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let docker = crate::docker::connect();
        let registry = Arc::new(crate::registry::load_registry());

        Self {
            docker,
            registry,
            data_dir,
            sessions: Arc::new(RwLock::new(HashSet::new())),
            login_attempts: Arc::new(RwLock::new(LoginAttempts::default())),
            tailscale_key: Arc::new(RwLock::new(None)),
            setup_lock: Arc::new(Mutex::new(())),
            ws_connections: Arc::new(RwLock::new(HashMap::new())),
            deploying: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create AppState with an explicit Docker connection (useful for testing).
    pub fn with_docker(docker: Option<Docker>, data_dir: PathBuf) -> Self {
        let registry = Arc::new(crate::registry::load_registry());
        Self {
            docker,
            registry,
            data_dir,
            sessions: Arc::new(RwLock::new(HashSet::new())),
            login_attempts: Arc::new(RwLock::new(LoginAttempts::default())),
            tailscale_key: Arc::new(RwLock::new(None)),
            setup_lock: Arc::new(Mutex::new(())),
            ws_connections: Arc::new(RwLock::new(HashMap::new())),
            deploying: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}
