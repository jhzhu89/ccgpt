# ccgpt

**Claude Code in front. GPT behind it.**

[![Release](https://img.shields.io/github/v/release/jhzhu89/ccgpt)](https://github.com/jhzhu89/ccgpt/releases/latest)
[![License](https://img.shields.io/github/license/jhzhu89/ccgpt)](LICENSE)

Keep the normal `claude` workflow while ccgpt routes requests to GPT through GitHub Copilot or any OpenAI-compatible Responses API.

ccgpt translates the Anthropic Messages API expected by Claude Code directly to the Responses API. It does not use Chat Completions.

- GitHub Copilot is the zero-config default backend.
- Claude model tiers map automatically to the latest available GPT tier.
- `claude --model` can select a tier or an exact backend model.
- Tool calls, reasoning continuity, streaming, and parallel tool use are preserved.

## Quick start

Requirements: [Bun](https://bun.sh), Git, and Claude Code CLI.

```bash
git clone https://github.com/jhzhu89/ccgpt.git
cd ccgpt
bun run setup
ccgpt auth
claude
```

`bun run setup` installs dependencies, builds ccgpt, links the local `ccgpt` command, and connects the `claude` shell command to ccgpt. Open a new shell after setup. It does not replace the Claude Code executable.

To select an exact backend model when needed:

```bash
claude --model gpt-5.6-sol
```

Run setup again after pulling code changes.

## GitHub Copilot

Copilot is the default backend. `ccgpt auth` authenticates with GitHub's device flow. The GitHub token is stored at `~/.local/share/ccgpt/github_token`, and short-lived Copilot tokens are refreshed automatically.

At startup, ccgpt reads the available Copilot Responses models and routes Claude model families by tier:

- Fable, Mythos, and Opus use the newest Sol model.
- Sonnet uses the newest Terra model.
- Haiku uses the newest Luna model.
- Unknown Claude model families use the Sol tier.

## API-key backend

Create `~/.ccgptrc`:

```bash
CCGPT_API_KEY=your-api-key
CCGPT_BASE_URL=https://api.openai.com/v1
CCGPT_MODEL_HIGH=gpt-5.6-sol
CCGPT_MODEL_BALANCED=gpt-5.6-terra
CCGPT_MODEL_FAST=gpt-5.6-luna
```

Only `CCGPT_API_KEY` is required. The base URL and model tiers shown above are the defaults. Generic OpenAI environment variables are intentionally ignored.

## Select a model

Claude Code remains the daily entry point. Its `--model` option selects a backend tier or an exact backend model:

```bash
claude --model opus
claude --model sonnet
claude --model haiku
claude --model gpt-5.6-sol
```

Omit `--model` to use Claude Code's default model; ccgpt maps it to the matching backend tier automatically.

With Copilot, exact model IDs must be advertised by the endpoint. With an API-key backend, non-Claude model IDs pass through unchanged.

Parallel tool calls are enabled only when the request contains tools, the selected model supports them, `tool_choice` is not `none`, and Claude Code has not disabled parallel tool use.

The shell integration starts the gateway on a free local port, configures Claude Code, forwards all arguments, and stops the gateway when Claude Code exits.

## Privacy and security

- The normal `claude` integration runs the gateway locally on `127.0.0.1` with a random free port.
- Prompts and tool results go directly from your machine to the selected GitHub Copilot or API-key backend. ccgpt has no hosted relay or telemetry.
- The GitHub token is stored locally with owner-only file permissions where the operating system supports them.
- API keys stay in `~/.ccgptrc`; do not commit that file.

## Run the gateway manually

Run `ccgpt` to listen on port 8000. Set `CCGPT_PORT` in `~/.ccgptrc` to change it, then configure Claude Code to use that address.

PowerShell:

```powershell
$env:ANTHROPIC_BASE_URL = "http://localhost:8000"
$env:ANTHROPIC_AUTH_TOKEN = "ccgpt"
claude
```

Bash or Zsh:

```bash
ANTHROPIC_BASE_URL=http://localhost:8000 \
ANTHROPIC_AUTH_TOKEN=ccgpt \
claude
```

## Verify locally

```bash
bun run lint
bun run test
bun run build
```

Live Copilot tests require `ccgpt auth`:

```bash
bun run test:integration
```

## Endpoints

- `POST /v1/messages`
- `POST /v1/messages/count_tokens`
- `GET /health`

## License

MIT
