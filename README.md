# m2r

Anthropic Messages API → Azure OpenAI Responses API proxy.
Enables Claude Code CLI and other Anthropic-compatible clients to use Azure OpenAI as the backend.

## Installation

```bash
npm install -g @jhzhu89/m2r
```

## Configuration

Create `~/.m2rrc` with your Azure OpenAI settings (Entra ID only; API keys are not used):

```bash
# Required
AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com

# Optional
AZURE_OPENAI_API_VERSION=2025-04-01-preview
PROXY_PORT=8000
LOG_LEVEL=info

# Model routing (optional)
MODEL_MAP={"claude-3-5-sonnet":"gpt-5.2"}
TIER_HAIKU=gpt-5-mini
TIER_SONNET=gpt-5.2
TIER_OPUS=gpt-5.1-codex-max
```

Auth uses `DefaultAzureCredential`, so ensure your environment is logged in (e.g., `az login`) or set the usual `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` / `AZURE_CLIENT_SECRET`.

Routing: `MODEL_MAP` overrides exact model aliases; otherwise `haiku`/`sonnet`/`opus` substrings map to the configured tier models.

## Usage

Start the proxy server:

```bash
m2r
```

Then point your Anthropic client to `http://localhost:8000`.

## Linux (systemd user service)

Install and start:

```bash
./scripts/m2r-service.sh install
```

Status and logs:

```bash
systemctl --user status m2r.service
journalctl --user -u m2r.service -f
```

Enable start on boot without login:

```bash
loginctl enable-linger $USER
```

Uninstall:

```bash
./scripts/m2r-service.sh uninstall
```

### Zsh / Bash (optional helpers)

Add to `~/.zshrc` or `~/.bashrc`:

```bash
claude() {
    local proxy_port=8000
    local m2rrc="$HOME/.m2rrc"

    if [[ -f "$m2rrc" ]]; then
        local port_line=$(grep '^PROXY_PORT=' "$m2rrc")
        if [[ -n "$port_line" ]]; then
            proxy_port="${port_line#PROXY_PORT=}"
        fi
    fi

    if ! nc -z localhost "$proxy_port" 2>/dev/null; then
        echo "Starting m2r on port $proxy_port..."
        mkdir -p "$HOME/.local/log"
        nohup m2r >> "$HOME/.local/log/m2r.log" 2>&1 &
        sleep 1
    fi

    ANTHROPIC_BASE_URL="http://localhost:$proxy_port" \
    ANTHROPIC_API_KEY="x" \
    CLAUDE_CODE_MAX_OUTPUT_TOKENS="64000" \
    command claude "$@"
}

m2r-config() {
    local m2rrc="$HOME/.m2rrc"
    local action="$1"
    local key="$2"
    local value="$3"

    mkdir -p "$(dirname "$m2rrc")"

    case "$action" in
        get)
            [[ -z "$key" ]] && echo "Usage: m2r-config get KEY" && return 1
            [[ -f "$m2rrc" ]] && grep -E "^${key}=" "$m2rrc" | tail -n 1 | cut -d= -f2-
            ;;
        set)
            [[ -z "$key" || -z "$value" ]] && echo "Usage: m2r-config set KEY VALUE" && return 1
            if [[ -f "$m2rrc" ]] && grep -q "^${key}=" "$m2rrc"; then
                if sed --version >/dev/null 2>&1; then
                    sed -i "s|^${key}=.*|${key}=${value}|" "$m2rrc"
                else
                    sed -i '' "s|^${key}=.*|${key}=${value}|" "$m2rrc"
                fi
            else
                echo "${key}=${value}" >> "$m2rrc"
            fi
            ;;
        list|"")
            [[ -f "$m2rrc" ]] && cat "$m2rrc" || true
            ;;
        *)
            echo "Usage: m2r-config [list|get KEY|set KEY VALUE]"
            return 1
            ;;
    esac
}
```

### PowerShell

Add to your `$PROFILE`:

```powershell
function Get-M2rPort {
    $m2rrc = "$HOME\.m2rrc"
    if (Test-Path $m2rrc) {
        switch -Regex -File $m2rrc { '^PROXY_PORT=(\d+)' { return [int]$Matches[1] } }
    }
    return 8000
}

function Test-M2rRunning($port) {
    try { $tcp = [System.Net.Sockets.TcpClient]::new("localhost", $port); $tcp.Dispose(); return $true } catch { return $false }
}

function Start-M2r($port) {
    $logDir = "$HOME\.local\log"
    New-Item -ItemType Directory -Path $logDir -Force -ErrorAction SilentlyContinue | Out-Null
    Start-Process powershell -ArgumentList "-WindowStyle Hidden -Command `"m2r *>> '$logDir\m2r.log'`"" -WindowStyle Hidden
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 250
        if (Test-M2rRunning $port) { return $true }
    }
    return $false
}

function claude {
    $port = Get-M2rPort
    if (-not (Test-M2rRunning $port)) {
        Write-Host "Starting m2r on port $port..." -ForegroundColor Cyan
        if (-not (Start-M2r $port)) { Write-Host "Failed to start m2r" -ForegroundColor Red; return }
    }
    $env:ANTHROPIC_BASE_URL = "http://localhost:$port"
    $env:ANTHROPIC_API_KEY = "x"
    $env:CLAUDE_CODE_MAX_OUTPUT_TOKENS = "64000"
    & (Get-Command claude -CommandType Application)[0].Source @args
}

function m2r-restart {
    $port = Get-M2rPort
    $stopped = $false
    Get-CimInstance Win32_Process -Filter "Name='bun.exe'" | Where-Object { $_.CommandLine -match 'm2r' } | ForEach-Object {
        Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
        $stopped = $true
    }
    Write-Host $(if ($stopped) { "Stopped m2r" } else { "m2r not running" })
    if (Start-M2r $port) { Write-Host "m2r started on port $port" } else { Write-Host "Failed to start m2r" -ForegroundColor Red }
}

function m2r-log {
    param([switch]$Follow, [int]$Tail = 50)
    $log = "$HOME\.local\log\m2r.log"
    if (-not (Test-Path $log)) {
        Write-Host "Log file not found: $log" -ForegroundColor Yellow
        return
    }
    if ($Follow) {
        Get-Content $log -Wait -Tail $Tail
    } else {
        Get-Content $log -Tail $Tail
    }
}
```

## License

MIT
