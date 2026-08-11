use std::fmt;
use std::future::Future;
use std::time::Duration;

use reqwest::{Client, Method, Response, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::CopilotTokenProvider;
use crate::models::CopilotModel;

const COPILOT_BASE_URL: &str = "https://api.githubcopilot.com";
const VSCODE_VERSION: &str = "1.131.0";
const COPILOT_CHAT_VERSION: &str = "0.26.7";
const CONNECTION_MISMATCH: &str = "input item does not belong to this connection";
const MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const CCGPT_UPSTREAM_REQUEST_ID_HEADER: &str = "x-ccgpt-upstream-request-id";

#[derive(Clone)]
pub enum Upstream {
    Copilot {
        client: Client,
        auth: CopilotTokenProvider,
        base_url: String,
    },
    Direct {
        client: Client,
        api_key: String,
        base_url: String,
    },
}

#[derive(Debug)]
pub struct UpstreamError {
    pub status: u16,
    pub message: String,
    pub request_id: Option<String>,
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for UpstreamError {}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<CopilotModel>,
}

impl Upstream {
    pub fn copilot(auth: CopilotTokenProvider) -> Result<Self, String> {
        Ok(Self::Copilot {
            client: http_client()?,
            auth,
            base_url: COPILOT_BASE_URL.into(),
        })
    }

    pub fn direct(api_key: String, base_url: String) -> Result<Self, String> {
        Ok(Self::Direct {
            client: http_client()?,
            api_key,
            base_url,
        })
    }

    pub async fn models(&self) -> Result<Vec<CopilotModel>, UpstreamError> {
        let Self::Copilot { .. } = self else {
            return Err(UpstreamError {
                status: 500,
                message: "Model discovery is only available for Copilot".into(),
                request_id: None,
            });
        };
        tokio::time::timeout(MODEL_DISCOVERY_TIMEOUT, async {
            let response = self.copilot_request(Method::GET, "/models", None).await?;
            let response = ensure_success(response).await?;
            response
                .json::<ModelsResponse>()
                .await
                .map(|body| body.data)
                .map_err(network_error)
        })
        .await
        .map_err(|_| UpstreamError {
            status: 504,
            message: "Copilot model discovery timed out".into(),
            request_id: None,
        })?
    }

    pub async fn responses(&self, body: &Value) -> Result<Response, UpstreamError> {
        match self {
            Self::Copilot {
                client,
                auth,
                base_url,
            } => {
                let token = auth.token().await.map_err(auth_error)?;
                send_copilot_responses(client, base_url, body, &token, || async {
                    auth.refresh().await.map_err(auth_error)
                })
                .await
            }
            Self::Direct {
                client,
                api_key,
                base_url,
            } => {
                let request_id = Uuid::new_v4().to_string();
                let response = client
                    .post(endpoint(base_url, "responses"))
                    .bearer_auth(api_key)
                    .header(header::ACCEPT, "application/json")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-request-id", &request_id)
                    .json(body)
                    .send()
                    .await
                    .map_err(|error| network_error_with_request_id(error, &request_id))?;
                ensure_success(tag_request_id(response, &request_id)).await
            }
        }
    }

    async fn copilot_request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, UpstreamError> {
        let Self::Copilot {
            client,
            auth,
            base_url,
        } = self
        else {
            unreachable!();
        };
        let token = auth.token().await.map_err(auth_error)?;
        let response = send_copilot(client, base_url, method.clone(), path, body, &token).await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        let token = auth.refresh().await.map_err(auth_error)?;
        send_copilot(client, base_url, method, path, body, &token).await
    }
}

async fn send_copilot_responses<F, Fut>(
    client: &Client,
    base_url: &str,
    body: &Value,
    token: &str,
    refresh: F,
) -> Result<Response, UpstreamError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String, UpstreamError>>,
{
    let response = send_copilot(
        client,
        base_url,
        Method::POST,
        "/responses",
        Some(body),
        token,
    )
    .await?;
    if response.status() != StatusCode::UNAUTHORIZED {
        return ensure_success(response).await;
    }

    let error = match ensure_success(response).await {
        Ok(response) => return Ok(response),
        Err(error) => error,
    };
    if is_connection_mismatch(&error) {
        let Some(body) = without_reasoning(body) else {
            return Err(error);
        };
        let response = send_copilot(
            client,
            base_url,
            Method::POST,
            "/responses",
            Some(&body),
            token,
        )
        .await?;
        return ensure_success(response).await;
    }

    let token = refresh().await?;
    let response = send_copilot(
        client,
        base_url,
        Method::POST,
        "/responses",
        Some(body),
        &token,
    )
    .await?;
    ensure_success(response).await
}

fn is_connection_mismatch(error: &UpstreamError) -> bool {
    error.status == StatusCode::UNAUTHORIZED.as_u16() && error.message == CONNECTION_MISMATCH
}

fn without_reasoning(body: &Value) -> Option<Value> {
    let mut body = body.clone();
    let input = body.get_mut("input")?.as_array_mut()?;
    let before = input.len();
    input.retain(|item| item.get("type").and_then(Value::as_str) != Some("reasoning"));
    (input.len() != before).then_some(body)
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())
}

async fn send_copilot(
    client: &Client,
    base_url: &str,
    method: Method,
    path: &str,
    body: Option<&Value>,
    token: &str,
) -> Result<Response, UpstreamError> {
    let request_id = Uuid::new_v4().to_string();
    let mut request = client
        .request(method, format!("{}{path}", base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .header("copilot-integration-id", "vscode-chat")
        .header("editor-version", format!("vscode/{VSCODE_VERSION}"))
        .header(
            "editor-plugin-version",
            format!("copilot-chat/{COPILOT_CHAT_VERSION}"),
        )
        .header("openai-intent", "conversation-panel")
        .header(
            "user-agent",
            format!("GitHubCopilotChat/{COPILOT_CHAT_VERSION}"),
        )
        .header("x-github-api-version", "2025-04-01")
        .header("x-initiator", "agent")
        .header("x-vscode-user-agent-library-version", "electron-fetch")
        .header("x-request-id", &request_id);
    if let Some(body) = body {
        request = request.json(body);
    }
    request
        .send()
        .await
        .map(|response| tag_request_id(response, &request_id))
        .map_err(|error| network_error_with_request_id(error, &request_id))
}

fn endpoint(base_url: &str, suffix: &str) -> String {
    format!("{}/{suffix}", base_url.trim_end_matches('/'))
}

async fn ensure_success(response: Response) -> Result<Response, UpstreamError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let request_id = response_request_id(response.headers());
    let text = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            let detail = text.chars().take(500).collect::<String>();
            if detail.trim().is_empty() {
                format!("Upstream request failed ({status})")
            } else {
                detail
            }
        });
    Err(UpstreamError {
        status,
        message,
        request_id,
    })
}

fn network_error(error: reqwest::Error) -> UpstreamError {
    UpstreamError {
        status: error.status().map_or(502, |status| status.as_u16()),
        message: error.to_string(),
        request_id: None,
    }
}

fn network_error_with_request_id(error: reqwest::Error, request_id: &str) -> UpstreamError {
    UpstreamError {
        status: error.status().map_or(502, |status| status.as_u16()),
        message: error.to_string(),
        request_id: Some(request_id.to_owned()),
    }
}

fn tag_request_id(mut response: Response, request_id: &str) -> Response {
    if let Ok(value) = header::HeaderValue::from_str(request_id) {
        response
            .headers_mut()
            .insert(CCGPT_UPSTREAM_REQUEST_ID_HEADER, value);
    }
    response
}

fn response_request_id(headers: &header::HeaderMap) -> Option<String> {
    ["x-request-id", CCGPT_UPSTREAM_REQUEST_ID_HEADER]
        .into_iter()
        .find_map(|name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
}

fn auth_error(error: impl fmt::Display) -> UpstreamError {
    UpstreamError {
        status: 401,
        message: error.to_string(),
        request_id: None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Json, Router, http::HeaderMap, response::IntoResponse, routing::post};
    use serde_json::json;
    use tokio::{net::TcpListener, sync::Mutex};

    use super::*;

    async fn server(app: Router) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn retries_connection_mismatch_once_without_reasoning() {
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let captured = attempts.clone();
        let app = Router::new().route(
            "/responses",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let captured = captured.clone();
                async move {
                    let authorization = headers[header::AUTHORIZATION].to_str().unwrap().to_owned();
                    let has_reasoning = body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["type"] == "reasoning");
                    captured.lock().await.push((authorization, body));
                    if has_reasoning {
                        (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"error":{"message":CONNECTION_MISMATCH}})),
                        )
                            .into_response()
                    } else {
                        (StatusCode::OK, "recovered").into_response()
                    }
                }
            }),
        );
        let refreshes = Arc::new(AtomicUsize::new(0));
        let refresh_count = refreshes.clone();
        let client = http_client().unwrap();
        let body = json!({
            "input": [
                {"type":"reasoning","encrypted_content":"old","summary":[]},
                {"type":"message","role":"user","content":"continue"}
            ]
        });

        let response = send_copilot_responses(
            &client,
            &server(app).await,
            &body,
            "original",
            move || async move {
                refresh_count.fetch_add(1, Ordering::SeqCst);
                Ok("refreshed".into())
            },
        )
        .await
        .unwrap();

        assert_eq!(response.text().await.unwrap(), "recovered");
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
        let attempts = attempts.lock().await;
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].0, "Bearer original");
        assert_eq!(attempts[1].0, "Bearer original");
        assert_eq!(attempts[0].1["input"].as_array().unwrap().len(), 2);
        assert_eq!(attempts[1].1["input"].as_array().unwrap().len(), 1);
        assert_eq!(attempts[1].1["input"][0]["type"], "message");
    }

    #[tokio::test]
    async fn does_not_retry_connection_mismatch_without_reasoning() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                count.fetch_add(1, Ordering::SeqCst);
                async {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error":{"message":CONNECTION_MISMATCH}})),
                    )
                }
            }),
        );
        let refreshes = Arc::new(AtomicUsize::new(0));
        let refresh_count = refreshes.clone();
        let client = http_client().unwrap();

        let error = send_copilot_responses(
            &client,
            &server(app).await,
            &json!({"input":[{"type":"message","role":"user","content":"hi"}]}),
            "original",
            move || async move {
                refresh_count.fetch_add(1, Ordering::SeqCst);
                Ok("refreshed".into())
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, 401);
        assert_eq!(error.message, CONNECTION_MISMATCH);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn refreshes_auth_without_stripping_reasoning_for_other_401s() {
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let captured = attempts.clone();
        let app = Router::new().route(
            "/responses",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let captured = captured.clone();
                async move {
                    let authorization = headers[header::AUTHORIZATION].to_str().unwrap().to_owned();
                    captured.lock().await.push((authorization.clone(), body));
                    if authorization == "Bearer original" {
                        (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"error":{"message":"token expired"}})),
                        )
                            .into_response()
                    } else {
                        (StatusCode::OK, "recovered").into_response()
                    }
                }
            }),
        );
        let refreshes = Arc::new(AtomicUsize::new(0));
        let refresh_count = refreshes.clone();
        let client = http_client().unwrap();
        let body = json!({"input":[{"type":"reasoning","encrypted_content":"old"}]});

        let response = send_copilot_responses(
            &client,
            &server(app).await,
            &body,
            "original",
            move || async move {
                refresh_count.fetch_add(1, Ordering::SeqCst);
                Ok("refreshed".into())
            },
        )
        .await
        .unwrap();

        assert_eq!(response.text().await.unwrap(), "recovered");
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        let attempts = attempts.lock().await;
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].0, "Bearer original");
        assert_eq!(attempts[1].0, "Bearer refreshed");
        assert_eq!(attempts[0].1, body);
        assert_eq!(attempts[1].1, body);
    }

    #[tokio::test]
    async fn does_not_retry_other_copilot_errors() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                count.fetch_add(1, Ordering::SeqCst);
                async {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        Json(json!({"error":{"message":"quota exhausted"}})),
                    )
                }
            }),
        );
        let refreshes = Arc::new(AtomicUsize::new(0));
        let refresh_count = refreshes.clone();
        let client = http_client().unwrap();

        let error = send_copilot_responses(
            &client,
            &server(app).await,
            &json!({"input":[{"type":"reasoning","encrypted_content":"old"}]}),
            "original",
            move || async move {
                refresh_count.fetch_add(1, Ordering::SeqCst);
                Ok("refreshed".into())
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, 429);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn direct_backend_does_not_retry_connection_mismatch() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                count.fetch_add(1, Ordering::SeqCst);
                async {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error":{"message":CONNECTION_MISMATCH}})),
                    )
                }
            }),
        );
        let upstream = Upstream::direct("secret".into(), server(app).await).unwrap();

        let error = upstream
            .responses(&json!({"input":[{"type":"reasoning","encrypted_content":"old"}]}))
            .await
            .unwrap_err();

        assert_eq!(error.status, 401);
        assert_eq!(error.message, CONNECTION_MISMATCH);
        assert!(error.request_id.is_some());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
