use std::ffi::OsString;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use ccgpt::auth::{
    CopilotTokenProvider, GithubDeviceFlow, default_github_token_path, save_github_token,
};
use ccgpt::config::Config;
use ccgpt::gateway::{Gateway, initialize};
use tokio::net::TcpListener;
use tokio::process::Command;

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ccgpt: {error}");
            1
        }
    };
    std::process::exit(code);
}

async fn run() -> Result<i32, String> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    match command.as_deref().and_then(std::ffi::OsStr::to_str) {
        Some("auth") => {
            authenticate().await?;
            Ok(0)
        }
        Some("setup") => {
            let profiles = ccgpt::setup::install()?;
            let profiles = profiles
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "Claude shell integration installed in {profiles}\nOpen a new shell and run claude."
            );
            Ok(0)
        }
        Some("run") => run_claude(args.collect()).await,
        Some("--help" | "-h" | "help") => {
            print_help();
            Ok(0)
        }
        Some("--version" | "-V") => {
            println!("ccgpt {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some(command) => Err(format!("unknown command: {command}")),
        None if command.is_some() => Err("command is not valid Unicode".into()),
        None => serve().await,
    }
}

async fn authenticate() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let flow = GithubDeviceFlow::new(client.clone());
    let device = flow
        .request_code()
        .await
        .map_err(|error| error.to_string())?;
    println!(
        "Open {} and enter code {}",
        device.verification_uri, device.user_code
    );
    let github_token = flow
        .poll_token(&device)
        .await
        .map_err(|error| error.to_string())?;
    let provider =
        CopilotTokenProvider::new(client, &github_token).map_err(|error| error.to_string())?;
    provider.token().await.map_err(|error| error.to_string())?;
    let path = default_github_token_path().map_err(|error| error.to_string())?;
    save_github_token(&path, &github_token).map_err(|error| error.to_string())?;
    println!("GitHub authentication saved to {}", path.display());
    Ok(())
}

async fn serve() -> Result<i32, String> {
    let config = Config::load().map_err(|error| error.to_string())?;
    let port = config.port;
    let gateway = initialize(&config).await?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    eprintln!(
        "ccgpt listening on http://{address} via {} ({}, {}, {})\nBearer token: {}",
        gateway.backend,
        gateway.targets.high,
        gateway.targets.balanced,
        gateway.targets.fast,
        gateway.auth_token
    );
    axum::serve(listener, gateway.app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| error.to_string())?;
    Ok(0)
}

async fn run_claude(args: Vec<OsString>) -> Result<i32, String> {
    let config = Config::load().map_err(|error| error.to_string())?;
    let gateway = initialize(&config).await?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let Gateway {
        app, auth_token, ..
    } = gateway;
    let mut server = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .map_err(|error| error.to_string())
    });
    let mut child = Command::new("claude")
        .args(args)
        .env("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))
        .env("ANTHROPIC_AUTH_TOKEN", auth_token)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_FOUNDRY_API_KEY")
        .env_remove("ANTHROPIC_FOUNDRY_BASE_URL")
        .env_remove("CLAUDE_CODE_USE_FOUNDRY")
        .env_remove("CCGPT_API_KEY")
        .env_remove("CCGPT_DEBUG_FILE")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("failed to start Claude Code: {error}"))?;

    let status = tokio::select! {
        result = child.wait() => result.map_err(|error| error.to_string())?,
        result = &mut server => {
            let _ = child.kill().await;
            let detail = match result {
                Ok(Ok(())) => "gateway stopped unexpectedly".to_string(),
                Ok(Err(error)) => format!("gateway stopped: {error}"),
                Err(error) => format!("gateway task failed: {error}"),
            };
            return Err(detail);
        }
    };
    server.abort();
    Ok(exit_code(status))
}

fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(1)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn print_help() {
    println!(
        "ccgpt {}\n\nUSAGE:\n  ccgpt              Start the gateway\n  ccgpt auth         Sign in to GitHub Copilot\n  ccgpt setup        Install the claude shell wrapper\n  ccgpt run [ARGS]   Run Claude Code through ccgpt",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::exit_code;

    #[test]
    fn maps_signal_to_shell_exit_code() {
        assert_eq!(exit_code(std::process::ExitStatus::from_raw(15)), 143);
    }
}
