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
```

### PowerShell

Add to your `$PROFILE`:

```powershell
function claude {
    $proxyPort = 8001
    $m2rrc = "$HOME\.m2rrc"

    if (Test-Path $m2rrc) {
        switch -Regex -File $m2rrc {
            '^PROXY_PORT=(\d+)' { $proxyPort = [int]$Matches[1] }
        }
    }

    $running = $false
    try {
        $tcp = [System.Net.Sockets.TcpClient]::new("localhost", $proxyPort)
        $tcp.Dispose()
        $running = $true
    } catch {}

    if (-not $running) {
        Write-Host "Starting m2r on port $proxyPort..." -ForegroundColor Cyan
        $logDir = "$HOME\.local\log"
        New-Item -ItemType Directory -Path $logDir -Force -ErrorAction SilentlyContinue | Out-Null
        Start-Process cmd -ArgumentList "/c m2r >> `"$logDir\m2r.log`" 2>&1" -WindowStyle Hidden

        for ($i = 0; $i -lt 20; $i++) {
            Start-Sleep -Milliseconds 250
            try {
                $tcp = [System.Net.Sockets.TcpClient]::new("localhost", $proxyPort)
                $tcp.Dispose()
                break
            } catch {}
        }
    }

    $exe = (Get-Command claude -CommandType Application -ErrorAction Stop)[0].Source

    $env:ANTHROPIC_BASE_URL = "http://localhost:$proxyPort"
    $env:ANTHROPIC_API_KEY = "x"
    $env:CLAUDE_CODE_MAX_OUTPUT_TOKENS = "64000"

    & $exe @args
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
