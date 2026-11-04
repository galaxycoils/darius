//! Proactive Nous token keepalive — background tokio task that refreshes the
//! credential ahead of expiry so long TUI/`serve` sessions never hit a mid-turn 401.
//!
//! Ported from NousResearch/hermes-agent @ `a6102b8d809d6b32824757b547b60f9d22d6a88e`
//! — `hermes_cli/nous_auth_keepalive.py`. Parameters: 15-min default tick,
//! 60s floor, tick derived from issued lifetime (4 ticks per lifetime — observed
//! lifetimes vary ~899s–3594s), 60s initial delay, refresh horizon = tick + skew
//! (wider than the request path's 120s skew so the keepalive, not the request
//! path, does the refreshing). Refresh failures log and back off to next tick;
//! the keepalive never blocks or kills the request path.
//!
//! Wiring: [`crate::runtime::SessionRuntime`] owns an `Option<Arc<Keepalive>>`
//! (armed only for `nous` providers) and starts it from the long-lived TUI
//! actor; `Drop` calls the sync [`Keepalive::shutdown`] (notify only — never
//! block on a runtime inside `Drop`).

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const DEFAULT_TICK_SECONDS: u64 = 900;
const FLOOR_SECONDS: u64 = 60;
const TICKS_PER_LIFETIME: u64 = 4;
const INITIAL_DELAY_SECONDS: u64 = 60;

pub struct Keepalive {
    auth: Arc<crate::auth::nous::NousAuthService>,
    tick: Duration,
    initial_delay: Duration,
    floor: Duration,
    notify: Arc<Notify>,
    handle: Arc<tokio::sync::RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Pure tick derivation shared by [`Keepalive::effective_tick`] and the
/// spawned task, so the schedule is unit-testable without wall-clock time.
fn derive_tick(expires_at: Option<i64>, now: i64, fallback: Duration, floor: Duration) -> Duration {
    match expires_at {
        Some(exp) => {
            let lifetime = (exp - now).max(1);
            Duration::from_secs((lifetime as u64 / TICKS_PER_LIFETIME).max(floor.as_secs().max(1)))
        }
        None => fallback,
    }
}

impl Keepalive {
    pub fn new(auth: Arc<crate::auth::nous::NousAuthService>) -> Self {
        let tick = std::env::var("NOUS_KEEPALIVE_INTERVAL_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v >= FLOOR_SECONDS)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(DEFAULT_TICK_SECONDS));
        Self {
            auth,
            tick,
            initial_delay: Duration::from_secs(INITIAL_DELAY_SECONDS),
            floor: Duration::from_secs(FLOOR_SECONDS),
            notify: Arc::new(Notify::new()),
            handle: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn with_tick_seconds(mut self, seconds: u64) -> Self {
        self.tick = Duration::from_secs(seconds.max(self.floor.as_secs().max(1)));
        self
    }

    /// Override the pre-first-tick delay (60s default). Tests set ~0.
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Override the tick floor (60s default). Tests set ~1s.
    pub fn with_floor(mut self, floor: Duration) -> Self {
        self.floor = floor;
        self
    }

    /// Effective sleep before the next refresh: lifetime-derived when a
    /// credential with `expires_at` exists, else the configured tick.
    pub fn effective_tick(&self, expires_at: Option<i64>, now: i64) -> Duration {
        derive_tick(expires_at, now, self.tick, self.floor)
    }

    pub async fn start(&self) {
        let mut handle = self.handle.write().await;
        if handle.is_some() {
            return;
        }
        let auth = self.auth.clone();
        let tick = self.tick;
        let floor = self.floor;
        let initial_delay = self.initial_delay;
        let notify = self.notify.clone();
        let h = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(initial_delay) => {},
                _ = notify.notified() => return,
            }
            loop {
                let expires_at = auth.status().ok().flatten().and_then(|s| s.expires_at);
                let effective =
                    derive_tick(expires_at, chrono::Utc::now().timestamp(), tick, floor);
                tokio::select! {
                    _ = tokio::time::sleep(effective) => {},
                    _ = notify.notified() => break,
                }
                match auth.refresh().await {
                    Ok(_) => tracing::info!("nous keepalive: token refreshed"),
                    Err(e) => tracing::warn!("nous keepalive: refresh failed: {e}"),
                }
            }
        });
        *handle = Some(h);
    }

    /// Synchronous shutdown: wakes the task at its next select point. Safe
    /// to call from `Drop` (no runtime required); the task then exits
    /// without refreshing again.
    pub fn shutdown(&self) {
        self.notify.notify_waiters();
    }

    pub async fn stop(&self) {
        self.shutdown();
        if let Some(h) = self.handle.write().await.take() {
            h.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::nous::{NousAuthConfig, NousAuthService, NousCred};
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn temp_home() -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!("darius-ka-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    fn service(home: &std::path::Path) -> Arc<NousAuthService> {
        Arc::new(NousAuthService::new(home).unwrap())
    }

    fn mock_service(home: &std::path::Path, server_uri: &str) -> Arc<NousAuthService> {
        let config = NousAuthConfig {
            portal_base: server_uri.to_string(),
            inference_base: server_uri.to_string(),
            scope: "inference:invoke".to_string(),
            client_id: "darius".to_string(),
        };
        Arc::new(NousAuthService::new(home).unwrap().with_config(config))
    }

    fn seed(service: &NousAuthService, expires_in_secs: i64) {
        service
            .save(&NousCred {
                access_token: "old-token".to_string(),
                refresh_token: Some("refresh-1".to_string()),
                expires_at: Some(chrono::Utc::now().timestamp() + expires_in_secs),
                scope: Some("inference:invoke".to_string()),
                token_type: Some("Bearer".to_string()),
                quarantined: false,
                quarantined_at: None,
                quarantine_reason: None,
            })
            .unwrap();
    }

    #[test]
    fn keepalive_tick_derives_from_token_lifetime() {
        let home = temp_home();
        let ka = Keepalive::new(service(&home));
        let now = chrono::Utc::now().timestamp();
        // 3600s lifetime -> 4 ticks -> 900s default tick.
        assert_eq!(
            ka.effective_tick(Some(now + 3600), now),
            Duration::from_secs(900)
        );
        // Short lifetime clamps to the 60s floor, never below.
        assert_eq!(
            ka.effective_tick(Some(now + 120), now),
            Duration::from_secs(60)
        );
        // Expired token still yields the floor, not zero/panic.
        assert_eq!(
            ka.effective_tick(Some(now - 30), now),
            Duration::from_secs(60)
        );
        // No credential -> configured tick.
        assert_eq!(ka.effective_tick(None, now), Duration::from_secs(900));
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn keepalive_tick_config_override_and_floor() {
        let home = temp_home();
        let ka = Keepalive::new(service(&home)).with_tick_seconds(120);
        let now = chrono::Utc::now().timestamp();
        assert_eq!(ka.effective_tick(None, now), Duration::from_secs(120));
        // Below-floor override clamps to 60s.
        let ka = Keepalive::new(service(&home)).with_tick_seconds(5);
        assert_eq!(ka.effective_tick(None, now), Duration::from_secs(60));
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn keepalive_start_is_idempotent_and_stop_clears() {
        let home = temp_home();
        let ka = Keepalive::new(service(&home));
        ka.start().await;
        ka.start().await;
        assert!(ka.handle.read().await.is_some());
        ka.stop().await;
        assert!(ka.handle.read().await.is_none());
        // Stopping twice is a no-op, not a panic.
        ka.stop().await;
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn keepalive_refresh_fires_ahead_of_expiry() {
        let home = temp_home();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "fresh-token",
                "refresh_token": "refresh-2",
                "expires_in": 3600,
                "token_type": "Bearer",
                "scope": "inference:invoke"
            })))
            .mount(&server)
            .await;
        let svc = mock_service(&home, &server.uri());
        seed(&svc, 4);
        let ka = Keepalive::new(svc.clone())
            .with_initial_delay(Duration::from_millis(50))
            .with_floor(Duration::from_secs(1))
            .with_tick_seconds(1);
        ka.start().await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        ka.stop().await;
        let stored = svc.load().unwrap().expect("credential stored");
        assert_eq!(stored.access_token, "fresh-token");
        let requests = server.received_requests().await.unwrap_or_default();
        assert!(
            requests.iter().any(|r| r.url.path() == "/oauth/token"),
            "expected a refresh POST, got {} requests",
            requests.len()
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn keepalive_failing_refresh_backs_off_without_panic() {
        let home = temp_home();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let svc = mock_service(&home, &server.uri());
        // 4s lifetime -> 4 ticks -> 1s (floor) so the refresh fires in the window.
        seed(&svc, 4);
        let ka = Keepalive::new(svc.clone())
            .with_initial_delay(Duration::from_millis(50))
            .with_floor(Duration::from_secs(1))
            .with_tick_seconds(1);
        ka.start().await;
        // Two tick windows: both refreshes fail, the task must survive.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(
            ka.handle.read().await.is_some(),
            "keepalive task died on refresh failure"
        );
        ka.stop().await;
        // The revoked credential was quarantined gracefully, not panicked on.
        let stored = svc.load().unwrap().expect("credential stored");
        assert!(stored.quarantined);
        std::fs::remove_dir_all(home).unwrap();
    }
}
