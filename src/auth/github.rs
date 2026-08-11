use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use super::{AuthError, REQUEST_TIMEOUT, decode, github_headers, require_success};

const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceCode {
    device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_poll_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

enum DevicePoll {
    Complete(String),
    Pending,
    SlowDown,
}

#[derive(Clone)]
pub struct GithubDeviceFlow {
    client: Client,
    device_code_url: String,
    access_token_url: String,
    timeout: Duration,
}

impl GithubDeviceFlow {
    pub fn new(client: Client) -> Self {
        Self::with_endpoints(
            client,
            GITHUB_DEVICE_CODE_URL,
            GITHUB_ACCESS_TOKEN_URL,
            REQUEST_TIMEOUT,
        )
    }

    fn with_endpoints(
        client: Client,
        device_code_url: impl Into<String>,
        access_token_url: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            device_code_url: device_code_url.into(),
            access_token_url: access_token_url.into(),
            timeout,
        }
    }

    pub async fn request_code(&self) -> Result<DeviceCode, AuthError> {
        let response = self
            .client
            .post(&self.device_code_url)
            .headers(github_headers(None)?)
            .json(&serde_json::json!({
                "client_id": GITHUB_CLIENT_ID,
                "scope": "read:user"
            }))
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|source| AuthError::Request {
                operation: "GitHub device code request",
                source,
            })?;
        let response = require_success(response, "GitHub device code request").await?;
        let device: DeviceCode = decode(response, "GitHub device code request").await?;
        if device.device_code.trim().is_empty()
            || device.user_code.trim().is_empty()
            || device.verification_uri.trim().is_empty()
            || device.expires_in == 0
            || device.interval == 0
        {
            return Err(AuthError::InvalidResponse {
                operation: "GitHub device code request",
                detail: "missing required device code fields".to_owned(),
            });
        }
        Ok(device)
    }

    pub async fn poll_token(&self, device: &DeviceCode) -> Result<String, AuthError> {
        let deadline = tokio::time::Instant::now()
            .checked_add(Duration::from_secs(device.expires_in))
            .ok_or_else(|| AuthError::InvalidResponse {
                operation: "GitHub device code request",
                detail: "expiry is out of range".to_owned(),
            })?;
        let mut interval = Duration::from_secs(device.interval);

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(AuthError::DeviceCodeExpired);
            }
            let next_poll = now.checked_add(interval).unwrap_or(deadline);
            tokio::time::sleep_until(next_poll.min(deadline)).await;

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(AuthError::DeviceCodeExpired);
            }
            let request_timeout = self.timeout.min(remaining);
            let poll = async {
                let response = self
                    .client
                    .post(&self.access_token_url)
                    .headers(github_headers(None)?)
                    .json(&serde_json::json!({
                        "client_id": GITHUB_CLIENT_ID,
                        "device_code": device.device_code,
                        "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
                    }))
                    .timeout(request_timeout)
                    .send()
                    .await
                    .map_err(|source| AuthError::Request {
                        operation: "GitHub device token request",
                        source,
                    })?;
                let response = require_success(response, "GitHub device token request").await?;
                decode(response, "GitHub device token request").await
            };
            let result: DeviceTokenResponse =
                match tokio::time::timeout(request_timeout, poll).await {
                    Ok(result) => result?,
                    Err(_) if tokio::time::Instant::now() >= deadline => {
                        return Err(AuthError::DeviceCodeExpired);
                    }
                    Err(_) => return Err(AuthError::TimedOut("GitHub device token request")),
                };

            match device_poll(result)? {
                DevicePoll::Complete(token) => return Ok(token),
                DevicePoll::Pending => {}
                DevicePoll::SlowDown => interval = interval.saturating_add(Duration::from_secs(5)),
            }
        }
    }
}

fn default_poll_interval() -> u64 {
    5
}

fn device_poll(response: DeviceTokenResponse) -> Result<DevicePoll, AuthError> {
    if let Some(token) = response.access_token {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return Ok(DevicePoll::Complete(token));
        }
    }
    match response.error.as_deref() {
        Some("authorization_pending") => Ok(DevicePoll::Pending),
        Some("slow_down") => Ok(DevicePoll::SlowDown),
        Some("expired_token") => Err(AuthError::DeviceCodeExpired),
        Some(code) => Err(AuthError::DeviceAuthorization {
            code: code.to_owned(),
            description: response.error_description,
        }),
        None => Err(AuthError::InvalidResponse {
            operation: "GitHub device token request",
            detail: "missing access_token or error".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Json, Router, routing::post};

    use super::*;

    #[test]
    fn classifies_device_poll_responses() {
        assert!(matches!(
            device_poll(DeviceTokenResponse {
                access_token: None,
                error: Some("authorization_pending".to_owned()),
                error_description: None,
            })
            .unwrap(),
            DevicePoll::Pending
        ));
        assert!(matches!(
            device_poll(DeviceTokenResponse {
                access_token: None,
                error: Some("slow_down".to_owned()),
                error_description: None,
            })
            .unwrap(),
            DevicePoll::SlowDown
        ));
        assert!(matches!(
            device_poll(DeviceTokenResponse {
                access_token: None,
                error: Some("access_denied".to_owned()),
                error_description: Some("denied".to_owned()),
            }),
            Err(AuthError::DeviceAuthorization { .. })
        ));
    }

    #[tokio::test]
    async fn completes_device_flow_after_pending_response() {
        let polls = Arc::new(AtomicUsize::new(0));
        let server_polls = polls.clone();
        let app = Router::new().route(
            "/token",
            post(move || {
                let server_polls = server_polls.clone();
                async move {
                    if server_polls.fetch_add(1, Ordering::SeqCst) == 0 {
                        Json(serde_json::json!({ "error": "authorization_pending" }))
                    } else {
                        Json(serde_json::json!({ "access_token": "github-token" }))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let flow = GithubDeviceFlow::with_endpoints(
            reqwest::Client::new(),
            format!("http://{address}/device"),
            format!("http://{address}/token"),
            Duration::from_secs(2),
        );
        let device = DeviceCode {
            device_code: "device".to_owned(),
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://github.com/login/device".to_owned(),
            expires_in: 5,
            interval: 0,
        };

        assert_eq!(flow.poll_token(&device).await.unwrap(), "github-token");
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn never_polls_an_expired_device_code() {
        let flow = GithubDeviceFlow::with_endpoints(
            reqwest::Client::new(),
            "http://127.0.0.1:1/device",
            "http://127.0.0.1:1/token",
            Duration::from_secs(1),
        );
        let device = DeviceCode {
            device_code: "device".to_owned(),
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://github.com/login/device".to_owned(),
            expires_in: 0,
            interval: 0,
        };

        assert!(matches!(
            flow.poll_token(&device).await,
            Err(AuthError::DeviceCodeExpired)
        ));
    }
}
