use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::{AuthError, REQUEST_TIMEOUT, decode, github_headers, require_success};

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const TOKEN_REFRESH_MARGIN: u64 = 60;

#[derive(Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: Option<u64>,
    refresh_in: Option<u64>,
}

struct CachedToken {
    value: String,
    refresh_at: Instant,
}

struct CopilotTokenProviderInner {
    client: Client,
    endpoint: String,
    github_token: String,
    timeout: Duration,
    cached: Mutex<Option<CachedToken>>,
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
                cached: Mutex::new(None),
            }),
        })
    }

    pub async fn token(&self) -> Result<String, AuthError> {
        self.resolve(false).await
    }

    pub async fn refresh(&self) -> Result<String, AuthError> {
        self.resolve(true).await
    }

    pub async fn invalidate(&self) {
        *self.inner.cached.lock().await = None;
    }

    async fn resolve(&self, force: bool) -> Result<String, AuthError> {
        tokio::time::timeout(self.inner.timeout, self.resolve_inner(force))
            .await
            .map_err(|_| AuthError::TimedOut("Copilot token exchange"))?
    }

    async fn resolve_inner(&self, force: bool) -> Result<String, AuthError> {
        let mut cached = self.inner.cached.lock().await;
        if !force
            && let Some(token) = cached
                .as_ref()
                .filter(|token| token.refresh_at > Instant::now())
        {
            return Ok(token.value.clone());
        }

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
        *cached = Some(CachedToken {
            value: value.clone(),
            refresh_at,
        });
        Ok(value)
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

    use axum::{Json, Router, routing::get};

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
}
