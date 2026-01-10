# m2r

Anthropic Messages API to Azure OpenAI Responses API proxy.

Enables Claude Code CLI and other Anthropic-compatible clients to use Azure OpenAI as the backend.

## Installation

```bash
npm install -g @jhzhu89/m2r
```

## Configuration

Create `~/.m2rrc` with your Azure OpenAI credentials:

```bash
AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com
AZURE_OPENAI_API_KEY=your-api-key
AZURE_OPENAI_DEPLOYMENT=your-deployment-name
PROXY_PORT=8001
LOG_LEVEL=info
```

## Usage

Start the proxy server:

```bash
m2r
```

Then point your Anthropic client to `http://localhost:8001`.

## Shell Integration

These shell functions automatically start `m2r` when you run `claude` and configure the necessary environment variables.

### Zsh / Bash

Add to `~/.zshrc` or `~/.bashrc`:

```bash
claude() {
    local proxy_port=8001
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

m2r-log() {
    local log="$HOME/.local/log/m2r.log"
    local follow=false tail=50
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -f|--follow) follow=true; shift ;;
            -n|--tail) tail="$2"; shift 2 ;;
            *) shift ;;
        esac
    done
    [[ ! -f "$log" ]] && echo "Log not found: $log" && return 1
    $follow && tail -n "$tail" -f "$log" || tail -n "$tail" "$log"
}

m2r-restart() {
    local proxy_port=8001 m2rrc="$HOME/.m2rrc"
    if [[ -f "$m2rrc" ]]; then
        local port_line=$(grep '^PROXY_PORT=' "$m2rrc")
        [[ -n "$port_line" ]] && proxy_port="${port_line#PROXY_PORT=}"
    fi
    pkill -f "node.*m2r" 2>/dev/null && echo "Stopped m2r" || echo "m2r not running"
    mkdir -p "$HOME/.local/log"
    nohup m2r >> "$HOME/.local/log/m2r.log" 2>&1 &
    for i in {1..10}; do
        sleep 0.3
        nc -z localhost "$proxy_port" 2>/dev/null && echo "m2r started on port $proxy_port" && return 0
    done
    echo "Failed to start m2r"; return 1
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
    return 8001
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
