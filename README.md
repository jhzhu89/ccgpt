# ccgpt

**Claude Code in front. GPT behind it.**

[![License](https://img.shields.io/github/license/jhzhu89/ccgpt)](LICENSE)

ccgpt lets you keep the normal `claude` workflow while GPT handles the work through GitHub Copilot or an OpenAI-compatible Responses API.

It translates Claude Code's Anthropic Messages requests directly to Responses. Chat Completions is not used.

- GitHub Copilot is the default backend when no API key is configured.
- Claude model families map automatically to the latest matching GPT tier.
- `claude --model` accepts a Claude tier or an exact backend model.
- Reasoning continuity, streaming, tools, images, and parallel tool calls are preserved.
- The temporary gateway stays silent and uses a random loopback port.

## Install

Windows and Linux are supported. Requirements: Rust 1.88 or newer, Git, the Claude Code CLI, and PowerShell, Bash, or Zsh.

```bash
git clone https://github.com/jhzhu89/ccgpt.git
cd ccgpt
cargo install --path . --locked --force
ccgpt setup
ccgpt auth
```

Open a new shell after `ccgpt setup`, then run:

```bash
claude
```

The setup command adds a small shell function so the daily command remains `claude`; it does not replace the Claude Code executable.

`ccgpt auth` prints a GitHub URL and device code. Open the URL on any device, enter the code, and return to the terminal.

After pulling an update, rebuild locally:

```bash
cargo install --path . --locked --force
```

## Models

At startup, the Copilot backend reads the available Responses models and selects the highest numeric version in each tier:

| Claude request | GPT tier |
| --- | --- |
| Opus, Fable, Mythos, default, or unknown | Sol |
| Sonnet | Terra |
| Haiku | Luna |

Omitting `--model` works normally. To override the backend model for one session:

```bash
claude --model sonnet
claude --model gpt-5.6-sol
```

Copilot accepts an exact model only when its model catalog advertises that ID. API-key backends pass non-Claude model IDs through unchanged.

With Copilot, Claude Code's requested reasoning effort is matched to the closest effort advertised by the selected model. Extended-thinking budgets map to low, medium, or high. Parallel tool calls are enabled only when the model advertises support and Claude Code has not disabled them. API-key backends are expected to expose current GPT Responses capabilities and receive these settings directly.

## API-key backend

Create `~/.ccgptrc`:

```bash
CCGPT_API_KEY=your-api-key
CCGPT_BASE_URL=https://api.openai.com/v1
CCGPT_MODEL_HIGH=gpt-5.6-sol
CCGPT_MODEL_BALANCED=gpt-5.6-terra
CCGPT_MODEL_FAST=gpt-5.6-luna
```

Only `CCGPT_API_KEY` is required. The other values shown are defaults. Generic OpenAI environment variables are intentionally ignored, so an unrelated `OPENAI_API_KEY` cannot silently change the backend.

Remove `CCGPT_API_KEY` to return to GitHub Copilot. Run `ccgpt auth` once to authorize Copilot through GitHub's device flow. The GitHub token is stored at `~/.local/share/ccgpt/github_token`; short-lived Copilot tokens are refreshed automatically.

## How it runs

The installed shell function turns:

```bash
claude <arguments>
```

into `ccgpt run <arguments>`. ccgpt binds a free `127.0.0.1` port, starts Claude Code with the two Anthropic endpoint variables, forwards its terminal and exit code, then shuts the gateway down.

Requests use `store: false`. Encrypted reasoning items are returned to Claude Code as opaque `ccgpt:` signatures and replayed on later turns, so reasoning context survives without server-side response state.

The local endpoints are:

- `POST /v1/messages`
- `POST /v1/messages/count_tokens`
- `GET /health`

To run the gateway manually on port 8000, use `ccgpt`. It prints the bearer token required as `ANTHROPIC_AUTH_TOKEN`. Set `CCGPT_PORT` in `~/.ccgptrc` to choose another port.

## Develop locally

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

There is no npm package or publishing pipeline.

## Privacy

- The normal wrapper listens only on a random loopback port.
- Prompts and tool results go directly to GitHub Copilot or the configured Responses endpoint.
- ccgpt has no hosted relay, telemetry, or conversation store.
- Secrets remain in local files and are never copied into the repository.

## History

ccgpt continues [m2r](https://github.com/jhzhu89/ccgpt/commit/c50bc1c2119972f926734d88c90cf854e3afafd8), first committed on January 5, 2026 and released as [v0.1.0](https://github.com/jhzhu89/ccgpt/tree/v0.1.0) the next day. The original commits and tags remain in this repository; ccgpt and this Rust rewrite are direct descendants, not squashed imports.

## License

[MIT](LICENSE)
