use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use eventsource_stream::Eventsource;
use futures_util::{StreamExt, pin_mut};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{CopilotTokenProvider, default_github_token_path, load_github_token};
use crate::config::{Backend, Config};
use crate::models::{ModelRouter, ModelTargets};
use crate::protocol::StreamTranslator;
use crate::protocol::{
    BuildOptions, anthropic_error, anthropic_response, build_responses_request, parse_request,
    parse_responses_response,
};
use crate::tokenizer::count_request;

use upstream::{Upstream, UpstreamError};

pub mod upstream;

#[derive(Clone)]
struct AppState {
    upstream: Upstream,
    models: Arc<ModelRouter>,
    auth_token: Arc<str>,
}

pub struct Gateway {
    pub app: Router,
    pub backend: &'static str,
    pub targets: ModelTargets,
    pub auth_token: String,
}

pub async fn initialize(config: &Config) -> Result<Gateway, String> {
    let (upstream, models, backend) = match &config.backend {
        Backend::Copilot => {
            let path = default_github_token_path().map_err(|error| error.to_string())?;
            let github_token = load_github_token(&path).map_err(|error| error.to_string())?;
            let auth = CopilotTokenProvider::new(http_client()?, github_token)
                .map_err(|error| error.to_string())?;
            let upstream = Upstream::copilot(auth)?;
            let catalog = upstream.models().await.map_err(|error| error.to_string())?;
            let models = ModelRouter::copilot(catalog).map_err(|error| error.to_string())?;
            (upstream, models, "copilot")
        }
        Backend::Direct {
            api_key,
            base_url,
            targets,
        } => (
            Upstream::direct(api_key.clone(), base_url.clone())?,
            ModelRouter::direct(targets.clone()),
            "direct",
        ),
    };
    let targets = models.targets.clone();
    let auth_token = Uuid::new_v4().simple().to_string();
    Ok(Gateway {
        app: app(upstream, models, auth_token.clone()),
        backend,
        targets,
        auth_token,
    })
}

pub fn app(upstream: Upstream, models: ModelRouter, auth_token: impl Into<String>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/messages", post(messages))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(AppState {
            upstream,
            models: Arc::new(models),
            auth_token: Arc::from(auth_token.into()),
        })
}

async fn health() -> impl IntoResponse {
    axum::Json(json!({ "status": "ok" }))
}

async fn count_tokens(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if !authorized(&headers, &state.auth_token) {
        return error_response(401, "authentication_error", "Invalid bearer token");
    }
    match parse_request(&body) {
        Ok(request) => {
            axum::Json(json!({ "input_tokens": count_request(&request) })).into_response()
        }
        Err(message) => error_response(400, "invalid_request_error", message),
    }
}

async fn messages(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let request_id = Uuid::new_v4().to_string();
    if !authorized(&headers, &state.auth_token) {
        return with_request_id(
            error_response(401, "authentication_error", "Invalid bearer token"),
            &request_id,
        );
    }
    let request = match parse_request(&body) {
        Ok(request) => request,
        Err(message) => {
            return with_request_id(
                error_response(400, "invalid_request_error", message),
                &request_id,
            );
        }
    };
    let resolved = match state.models.resolve(&request.model) {
        Ok(model) => model,
        Err(error) => {
            return with_request_id(
                error_response(400, "invalid_request_error", error.to_string()),
                &request_id,
            );
        }
    };
    let upstream_body = build_responses_request(
        &request,
        BuildOptions {
            model: &resolved.model,
            supports_parallel_tool_calls: resolved.capabilities.supports_parallel_tool_calls,
            reasoning_efforts: &resolved.capabilities.reasoning_efforts,
        },
    );
    let upstream = match state.upstream.responses(&upstream_body).await {
        Ok(response) => response,
        Err(error) => return with_request_id(upstream_error_response(error), &request_id),
    };

    if request.stream {
        return with_request_id(streaming_response(upstream, request.model), &request_id);
    }

    let response = match upstream.json::<Value>().await {
        Ok(value) => value,
        Err(error) => {
            return with_request_id(
                error_response(502, "api_error", error.to_string()),
                &request_id,
            );
        }
    };
    let response = match parse_responses_response(&response) {
        Ok(response) => response,
        Err(error) => return with_request_id(error_response(502, "api_error", error), &request_id),
    };
    with_request_id(
        axum::Json(anthropic_response(response, &request.model)).into_response(),
        &request_id,
    )
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .is_some_and(|(scheme, token)| scheme.eq_ignore_ascii_case("bearer") && token == expected)
}

fn streaming_response(upstream: reqwest::Response, requested_model: String) -> Response {
    let output = stream! {
        let events = upstream.bytes_stream().eventsource();
        pin_mut!(events);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        let mut translator = StreamTranslator::new(requested_model);

        loop {
            tokio::select! {
                next = events.next() => match next {
                    Some(Ok(event)) if event.data == "[DONE]" => {
                        if !translator.is_terminal() {
                            let frames = translator.abort("Upstream stream ended before completion");
                            if !frames.is_empty() {
                                yield Ok(Bytes::from(frames.concat()));
                            }
                        }
                        break;
                    },
                    Some(Ok(event)) => match serde_json::from_str::<Value>(&event.data) {
                        Ok(event) => {
                            let frames = translator.push(&event);
                            if !frames.is_empty() {
                                yield Ok::<Bytes, Infallible>(Bytes::from(frames.concat()));
                            }
                        }
                        Err(error) => {
                            let frames = translator.abort(error.to_string());
                            if !frames.is_empty() {
                                yield Ok(Bytes::from(frames.concat()));
                            }
                            break;
                        }
                    },
                    Some(Err(error)) => {
                        let frames = translator.abort(error.to_string());
                        if !frames.is_empty() {
                            yield Ok(Bytes::from(frames.concat()));
                        }
                        break;
                    }
                    None => {
                        if !translator.is_terminal() {
                            let frames = translator.abort("Upstream stream ended before completion");
                            if !frames.is_empty() {
                                yield Ok(Bytes::from(frames.concat()));
                            }
                        }
                        break;
                    },
                },
                _ = heartbeat.tick() => {
                    yield Ok(Bytes::from_static(b": keep-alive\n\n"));
                }
            }
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(output))
        .expect("valid streaming response")
}

fn upstream_error_response(error: UpstreamError) -> Response {
    let error_type = match error.status {
        400 => "invalid_request_error",
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        429 => "rate_limit_error",
        529 => "overloaded_error",
        _ => "api_error",
    };
    let status = if error.status == 529 {
        529
    } else if error.status >= 500 {
        502
    } else {
        error.status
    };
    error_response(status, error_type, error.message)
}

fn error_response(status: u16, error_type: &str, message: impl Into<String>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, axum::Json(anthropic_error(error_type, message))).into_response()
}

fn with_request_id(mut response: Response, request_id: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::to_bytes;
    use axum::http::{HeaderMap, Request};
    use axum::routing::post;
    use futures_util::stream;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use super::*;

    async fn upstream(app: Router) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}/v1")
    }

    fn direct_app(base_url: String) -> Router {
        app(
            Upstream::direct("secret".into(), base_url).unwrap(),
            ModelRouter::direct(ModelTargets::default()),
            "test-token",
        )
    }

    async fn call(app: Router, body: Value) -> Response {
        app.oneshot(
            Request::post("/v1/messages")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer test-token")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn rejects_requests_without_the_gateway_token() {
        let fake = Router::new().route(
            "/v1/responses",
            post(|| async { axum::Json(json!({ "output": [] })) }),
        );
        let app = direct_app(upstream(fake).await);

        for path in ["/v1/messages", "/v1/messages/count_tokens"] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(path)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(r#"{"model":"claude","messages":[]}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[test]
    fn maps_upstream_errors_without_losing_the_message() {
        let response = upstream_error_response(UpstreamError {
            status: 429,
            message: "quota exhausted".into(),
            request_id: Some("upstream".into()),
        });
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn translates_a_non_streaming_request_end_to_end() {
        let captured = Arc::new(Mutex::new(None));
        let capture = captured.clone();
        let fake = Router::new().route(
            "/v1/responses",
            post(
                move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
                    let capture = capture.clone();
                    async move {
                        assert_eq!(headers[header::AUTHORIZATION], "Bearer secret");
                        *capture.lock().await = Some(body);
                        axum::Json(json!({
                            "status": "completed",
                            "output": [{
                                "type": "message",
                                "content": [{"type":"output_text","text":"hello"}]
                            }],
                            "usage": {"input_tokens": 12, "output_tokens": 3}
                        }))
                    }
                },
            ),
        );
        let app = direct_app(upstream(fake).await);
        let response = call(
            app,
            json!({
                "model": "claude-opus-4-1",
                "messages": [{"role":"user","content":"hi"}],
                "tools": [{"name":"read","input_schema":{"type":"object"}}],
                "output_config": {"effort":"max"},
                "max_tokens": 100
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(response["content"][0]["text"], "hello");
        assert_eq!(response["model"], "claude-opus-4-1");
        assert_eq!(response["usage"]["input_tokens"], 12);

        let request = captured.lock().await.take().unwrap();
        assert_eq!(request["model"], "gpt-5.6-sol");
        assert_eq!(request["store"], false);
        assert_eq!(request["reasoning"]["effort"], "max");
        assert_eq!(request["parallel_tool_calls"], true);
    }

    #[tokio::test]
    async fn preserves_upstream_error_status_and_message() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = attempts.clone();
        let fake = Router::new().route(
            "/v1/responses",
            post(move || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        axum::Json(json!({"error":{"message":"quota exhausted"}})),
                    )
                }
            }),
        );
        let response = call(
            direct_app(upstream(fake).await),
            json!({"model":"claude-opus","messages":[]}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["error"]["type"], "rate_limit_error");
        assert_eq!(body["error"]["message"], "quota exhausted");
    }

    #[tokio::test]
    async fn parses_fragmented_sse_and_interleaved_tools() {
        let sse = concat!(
            "event: response.created\r\n",
            "data: {\"type\":\r\n",
            "data: \"response.created\"}\r\n\r\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"item_a\",\"type\":\"function_call\",\"call_id\":\"call_a\",\"name\":\"read_a\"}}\n\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"item_b\",\"type\":\"function_call\",\"call_id\":\"call_b\",\"name\":\"read_b\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item_a\",\"delta\":\"{\\\"a\\\":\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item_b\",\"delta\":\"{\\\"b\\\":2}\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item_a\",\"delta\":\"1}\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"item_a\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"item_b\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"output\":[],\"usage\":{\"output_tokens\":8}}}\n\n"
        );
        let chunks = [&sse[..17], &sse[17..91], &sse[91..219], &sse[219..]]
            .into_iter()
            .map(|chunk| Ok::<_, Infallible>(Bytes::copy_from_slice(chunk.as_bytes())));
        let fake = Router::new().route(
            "/v1/responses",
            post(move || async {
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::from_stream(stream::iter(chunks)))
                    .unwrap()
            }),
        );
        let response = call(
            direct_app(upstream(fake).await),
            json!({"model":"claude-opus","messages":[],"stream":true}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("\"id\":\"call_a\""));
        assert!(body.contains("\"id\":\"call_b\""));
        assert!(body.contains("\"index\":0"));
        assert!(body.contains("\"index\":1"));
        assert!(body.contains("\"stop_reason\":\"tool_use\""));
        assert!(body.contains("event: message_stop"));
        assert!(!body.contains("event: error"));
    }
}
