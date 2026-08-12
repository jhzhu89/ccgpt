use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{Mutex, watch};

use super::{AuthError, REQUEST_TIMEOUT, decode, github_headers, require_success};

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const TOKEN_REFRESH_MARGIN: u64 = 60;
const TOKEN_REFRESH_ATTEMPTS: usize = 3;
const TOKEN_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: Option<u64>,
    refresh_in: Option<u64>,
}

#[derive(Clone)]
struct CachedToken {
    value: String,
    refresh_at: Instant,
    refresh_after: Duration,
}

type RefreshResult = Result<CachedToken, Arc<AuthError>>;
type RefreshReceiver = watch::Receiver<Option<RefreshResult>>;

struct TokenState {
    cached: Option<CachedToken>,
    in_flight: Option<RefreshReceiver>,
}

struct CopilotTokenProviderInner {
    client: Client,
    endpoint: String,
    github_token: String,
    timeout: Duration,
    retry_delay: Duration,
    state: Mutex<TokenState>,
    refresh_started: AtomicBool,
}

#[derive(Clone)]
pub struct CopilotTokenProvider {
    inner: Arc<CopilotTokenProviderInner>,
}

impl CopilotTokenProvider {
    pub fn new(client: Client, github_token: impl Into<String>) -> Result<Self, AuthError> {
        Self::with_endpoint(client, COPILOT_TOKEN_URL, github_token, REQUEST_TIMEOUT)
    }

    fn with_endpoint(
        client: Client,
        endpoint: impl Into<String>,
        github_token: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, AuthError> {
        Self::with_settings(client, endpoint, github_token, timeout, TOKEN_RETRY_DELAY)
    }

    fn with_settings(
        client: Client,
        endpoint: impl Into<String>,
        github_token: impl Into<String>,
        timeout: Duration,
        retry_delay: Duration,
    ) -> Result<Self, AuthError> {
        let github_token = github_token.into().trim().to_owned();
        if github_token.is_empty() {
            return Err(AuthError::InvalidToken("GitHub"));
        }
        github_headers(Some(&github_token))?;
        Ok(Self {
            inner: Arc::new(CopilotTokenProviderInner {
                client,
                endpoint: endpoint.into(),
                github_token,
                timeout,
                retry_delay,
                state: Mutex::new(TokenState {
                    cached: None,
                    in_flight: None,
                }),
                refresh_started: AtomicBool::new(false),
            }),
        })
    }

    pub async fn initialize(&self) -> Result<(), AuthError> {
        self.resolve(false, 1).await?;
        if self
            .inner
            .refresh_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.spawn_refresh_loop().await;
        }
        Ok(())
    }

    pub async fn token(&self) -> Result<String, AuthError> {
        self.resolve(false, TOKEN_REFRESH_ATTEMPTS).await
    }

    pub async fn refresh(&self) -> Result<String, AuthError> {
        self.resolve(true, TOKEN_REFRESH_ATTEMPTS).await
    }

    pub async fn invalidate(&self) {
        self.inner.state.lock().await.cached = None;
    }

    async fn resolve(&self, force: bool, attempts: usize) -> Result<String, AuthError> {
        let mut receiver = {
            let mut state = self.inner.state.lock().await;
            if !force
                && let Some(token) = state
                    .cached
                    .as_ref()
                    .filter(|token| token.refresh_at > Instant::now())
            {
                return Ok(token.value.clone());
            }
            if let Some(receiver) = &state.in_flight {
                receiver.clone()
            } else {
                let (sender, receiver) = watch::channel(None);
                state.in_flight = Some(receiver.clone());
                let provider = self.clone();
                tokio::spawn(async move {
                    provider.finish_refresh(sender, attempts).await;
                });
                receiver
            }
        };
        let result = receiver
            .wait_for(Option::is_some)
            .await
            .map_err(|_| AuthError::InvalidResponse {
                operation: "Copilot token exchange",
                detail: "refresh task stopped".to_owned(),
            })?
            .clone()
            .expect("refresh result is present");
        match result {
            Ok(token) => Ok(token.value),
            Err(error) => Err(AuthError::Shared(error)),
        }
    }

    async fn finish_refresh(&self, sender: watch::Sender<Option<RefreshResult>>, attempts: usize) {
        let result = self.exchange_with_retry(attempts).await.map_err(Arc::new);
        let mut state = self.inner.state.lock().await;
        if let Ok(token) = &result {
            state.cached = Some(token.clone());
        }
        sender.send_replace(Some(result));
        state.in_flight = None;
    }

    async fn exchange_with_retry(&self, attempts: usize) -> Result<CachedToken, AuthError> {
        let mut last_error = None;
        for attempt in 1..=attempts {
            match self.exchange().await {
                Ok(token) => return Ok(token),
                Err(error) => last_error = Some(error),
            }
            if attempt < attempts {
                tokio::time::sleep(self.inner.retry_delay.saturating_mul(attempt as u32)).await;
            }
        }
        Err(last_error.expect("at least one token exchange attempt"))
    }

    async fn exchange(&self) -> Result<CachedToken, AuthError> {
        let response = self
            .inner
            .client
            .get(&self.inner.endpoint)
            .headers(github_headers(Some(&self.inner.github_token))?)
            .timeout(self.inner.timeout)
            .send()
            .await
            .map_err(|source| AuthError::Request {
                operation: "Copilot token exchange",
                source,
            })?;
        let response = require_success(response, "Copilot token exchange").await?;
        let token: CopilotTokenResponse = decode(response, "Copilot token exchange").await?;
        let value = token.token.trim().to_owned();
        if value.is_empty() {
            return Err(AuthError::InvalidToken("Copilot"));
        }
        let refresh_after = refresh_after(&token, unix_time()?)?;
        let refresh_at = Instant::now().checked_add(refresh_after).ok_or_else(|| {
            AuthError::InvalidResponse {
                operation: "Copilot token exchange",
                detail: "expiry is out of range".to_owned(),
            }
        })?;
        Ok(CachedToken {
            value,
            refresh_at,
            refresh_after,
        })
    }

    async fn spawn_refresh_loop(&self) {
        let delay = self
            .inner
            .state
            .lock()
            .await
            .cached
            .as_ref()
            .map(|token| token.refresh_after)
            .unwrap_or(Duration::from_secs(1));
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut delay = delay;
            loop {
                tokio::time::sleep(delay).await;
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let provider = CopilotTokenProvider { inner };
                match provider.refresh().await {
                    Ok(_) => {
                        if let Some(next) = provider
                            .inner
                            .state
                            .lock()
                            .await
                            .cached
                            .as_ref()
                            .map(|token| token.refresh_after)
                        {
                            delay = next;
                        }
                    }
                    Err(error) => eprintln!("ccgpt: Copilot token refresh failed: {error}"),
                }
            }
        });
    }
}

fn refresh_after(response: &CopilotTokenResponse, now: u64) -> Result<Duration, AuthError> {
    let until_expiry = response
        .expires_at
        .map(|expires_at| expires_at.saturating_sub(now));
    let ttl = match (until_expiry, response.refresh_in) {
        (Some(expires), Some(refresh)) => expires.min(refresh),
        (Some(expires), None) => expires,
        (None, Some(refresh)) => refresh,
        (None, None) => {
            return Err(AuthError::InvalidResponse {
                operation: "Copilot token exchange",
                detail: "missing expires_at and refresh_in".to_owned(),
            });
        }
    };
    if ttl == 0 {
        return Err(AuthError::InvalidResponse {
            operation: "Copilot token exchange",
            detail: "token is already expired".to_owned(),
        });
    }
    let seconds = ttl.saturating_sub(TOKEN_REFRESH_MARGIN).max(ttl / 2).max(1);
    Ok(Duration::from_secs(seconds))
}

fn unix_time() -> Result<u64, AuthError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AuthError::Clock)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::get};

    use super::*;

    #[test]
    fn derives_conservative_refresh_time() {
        let response = CopilotTokenResponse {
            token: "token".to_owned(),
            expires_at: Some(1_300),
            refresh_in: Some(100),
        };
        assert_eq!(refresh_after(&response, 1_000).unwrap().as_secs(), 50);

        let response = CopilotTokenResponse {
            token: "token".to_owned(),
            expires_at: None,
            refresh_in: Some(600),
        };
        assert_eq!(refresh_after(&response, 1_000).unwrap().as_secs(), 540);
    }

    #[test]
    fn rejects_expired_or_unbounded_tokens() {
        let expired = CopilotTokenResponse {
            token: "token".to_owned(),
            expires_at: Some(1_000),
            refresh_in: None,
        };
        assert!(matches!(
            refresh_after(&expired, 1_000),
            Err(AuthError::InvalidResponse { .. })
        ));

        let unbounded = CopilotTokenResponse {
            token: "token".to_owned(),
            expires_at: None,
            refresh_in: None,
        };
        assert!(matches!(
            refresh_after(&unbounded, 1_000),
            Err(AuthError::InvalidResponse { .. })
        ));
    }

    #[tokio::test]
    async fn caches_refreshes_and_invalidates_copilot_tokens() {
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let app = Router::new().route(
            "/token",
            get(move || {
                let server_hits = server_hits.clone();
                async move {
                    let hit = server_hits.fetch_add(1, Ordering::SeqCst) + 1;
                    Json(serde_json::json!({
                        "token": format!("copilot-{hit}"),
                        "refresh_in": 600
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = CopilotTokenProvider::with_endpoint(
            reqwest::Client::new(),
            format!("http://{address}/token"),
            "github-token",
            Duration::from_secs(2),
        )
        .unwrap();

        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let provider = provider.clone();
            requests.spawn(async move { provider.token().await.unwrap() });
        }
        while let Some(result) = requests.join_next().await {
            assert_eq!(result.unwrap(), "copilot-1");
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(provider.refresh().await.unwrap(), "copilot-2");
        assert_eq!(provider.token().await.unwrap(), "copilot-2");
        provider.invalidate().await;
        assert_eq!(provider.token().await.unwrap(), "copilot-3");
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn initializes_and_refreshes_in_background() {
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let app = Router::new().route(
            "/token",
            get(move || {
                let server_hits = server_hits.clone();
                async move {
                    let hit = server_hits.fetch_add(1, Ordering::SeqCst) + 1;
                    Json(serde_json::json!({
                        "token": format!("copilot-{hit}"),
                        "refresh_in": 2
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = CopilotTokenProvider::with_settings(
            reqwest::Client::new(),
            format!("http://{address}/token"),
            "github-token",
            Duration::from_secs(2),
            Duration::from_millis(1),
        )
        .unwrap();

        provider.initialize().await.unwrap();
        assert_eq!(provider.token().await.unwrap(), "copilot-1");
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if provider.token().await.unwrap() == "copilot-2" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retries_and_deduplicates_refreshes() {
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let app = Router::new().route(
            "/token",
            get(move || {
                let server_hits = server_hits.clone();
                async move {
                    let hit = server_hits.fetch_add(1, Ordering::SeqCst) + 1;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    if matches!(hit, 2 | 3) {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        Json(serde_json::json!({
                            "token": format!("copilot-{hit}"),
                            "refresh_in": 600
                        }))
                        .into_response()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = CopilotTokenProvider::with_settings(
            reqwest::Client::new(),
            format!("http://{address}/token"),
            "github-token",
            Duration::from_secs(2),
            Duration::from_millis(1),
        )
        .unwrap();
        assert_eq!(provider.token().await.unwrap(), "copilot-1");

        let mut refreshes = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let provider = provider.clone();
            refreshes.spawn(async move { provider.refresh().await.unwrap() });
        }
        while let Some(result) = refreshes.join_next().await {
            assert_eq!(result.unwrap(), "copilot-4");
        }
        assert_eq!(hits.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn preserves_cached_token_after_failed_refresh() {
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let app = Router::new().route(
            "/token",
            get(move || {
                let server_hits = server_hits.clone();
                async move {
                    let hit = server_hits.fetch_add(1, Ordering::SeqCst) + 1;
                    if hit == 1 {
                        Json(serde_json::json!({
                            "token": "copilot-1",
                            "refresh_in": 600
                        }))
                        .into_response()
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = CopilotTokenProvider::with_settings(
            reqwest::Client::new(),
            format!("http://{address}/token"),
            "github-token",
            Duration::from_secs(2),
            Duration::from_millis(1),
        )
        .unwrap();
        assert_eq!(provider.token().await.unwrap(), "copilot-1");

        assert!(provider.refresh().await.is_err());
        assert_eq!(provider.token().await.unwrap(), "copilot-1");
        assert_eq!(hits.load(Ordering::SeqCst), 4);
    }
}
