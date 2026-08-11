use std::{env, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
    response::Response,
    routing::post,
};
use tokio::{net::TcpListener, process::Command, time::timeout};
use uuid::Uuid;

const PARTIAL_TEXT: &str =
    "先暂停替换：仓库已经 clone 到本地，但旧 Skill 还没有删除。你这个判断点很关键";
const UPSTREAM_ERROR: &str = "Synthetic upstream server failure after partial output";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let app = Router::new().route("/v1/responses", post(failed_stream));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let binary = env::current_dir()?
        .join("target/release")
        .join(format!("ccgpt{}", env::consts::EXE_SUFFIX));
    let diagnostic_dir = env::temp_dir().join(format!("ccgpt-midstream-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&diagnostic_dir)?;
    let debug_file = diagnostic_dir.join("claude-debug.log");
    let gateway_debug_file = diagnostic_dir.join("ccgpt-debug.jsonl");

    eprintln!("diagnostic directory: {}", diagnostic_dir.display());
    eprintln!("fake upstream: http://{address}/v1");

    let mut child = Command::new(binary);
    child
        .args([
            "run",
            "-p",
            "--verbose",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--permission-mode",
            "bypassPermissions",
            "--debug-file",
        ])
        .arg(&debug_file)
        .arg("触发诊断响应")
        .env("CCGPT_API_KEY", "diagnostic-only")
        .env("CCGPT_BASE_URL", format!("http://{address}/v1"))
        .env("CCGPT_DEBUG_FILE", &gateway_debug_file)
        .kill_on_drop(true);

    let output = timeout(Duration::from_secs(90), child.output()).await??;
    server.abort();

    println!("exit: {}", output.status);
    println!("--- stdout ---");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("--- stderr ---");
    println!("{}", String::from_utf8_lossy(&output.stderr));
    println!("--- ccgpt diagnostics ---");
    println!("{}", std::fs::read_to_string(gateway_debug_file)?);

    Ok(())
}

async fn failed_stream() -> Response {
    let body = [
        format!(
            "data: {}\n\n",
            serde_json::json!({
                "type": "response.created",
                "response": { "id": "resp_synthetic", "status": "in_progress" }
            })
        ),
        format!(
            "data: {}\n\n",
            serde_json::json!({
                "type": "response.output_text.delta",
                "output_index": 0,
                "content_index": 0,
                "delta": PARTIAL_TEXT
            })
        ),
        format!(
            "data: {}\n\n",
            serde_json::json!({
                "type": "response.failed",
                "response": {
                    "id": "resp_synthetic",
                    "status": "failed",
                    "error": {
                        "code": "server_error",
                        "message": UPSTREAM_ERROR
                    }
                }
            })
        ),
    ]
    .concat();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-request-id", "synthetic-upstream-request")
        .body(Body::from(body))
        .expect("valid synthetic SSE response")
}
