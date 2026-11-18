<#
.SYNOPSIS
    Start the DhakaFlix relay (foreground by default, or detached).

.DESCRIPTION
    Starts the addon + streaming relay. Unless a named tunnel is configured via
    TUNNEL_TOKEN in .env, a temporary https://*.trycloudflare.com URL is created.

.PARAMETER Background
    Run detached with logs in .\logs\relay.log and exit the console.

.PARAMETER Open
    Open the install page in the default browser once the URL is known.

.PARAMETER Port
    Override the port from .env / the default 7001.

.EXAMPLE
    .\scripts\start.ps1 -Open

.EXAMPLE
    .\scripts\start.ps1 -Background
#>

[CmdletBinding()]
param(
    [switch]$Background,
    [switch]$Open,
    [int]$Port = 0
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

function Write-Step($message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Fail($message) { Write-Host "`n[ERROR] $message" -ForegroundColor Red; exit 1 }

# --- preflight -------------------------------------------------------------
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Fail 'Node.js not found. Run setup.cmd first.'
}
if (-not (Test-Path (Join-Path $root 'node_modules'))) {
    Fail 'Dependencies are missing. Run setup.cmd first.'
}
if (-not (Test-Path (Join-Path $root '.env'))) {
    Write-Host 'No .env found, running setup...' -ForegroundColor Yellow
    & (Join-Path $root 'scripts\setup.ps1')
}

# Resolve the port: parameter > .env > default
$envFile = Join-Path $root '.env'
$envValues = @{}
if (Test-Path $envFile) {
    foreach ($line in Get-Content $envFile) {
        if ($line -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$' -and $line -notmatch '^\s*#') {
            $envValues[$Matches[1].ToUpper()] = $Matches[2].Trim().Trim('"').Trim("'")
        }
    }
}

if ($Port -le 0) {
    $Port = 7001
    if ($envValues['PORT']) { $Port = [int]$envValues['PORT'] }
}

# Only request the quick tunnel when nothing else is configured. The desktop
# app's Tailscale Funnel mode writes PUBLIC_URL=https://<host>.<tailnet>.ts.net
# with TUNNEL=0 and no token; forcing --tunnel there would start a stray
# cloudflared quick tunnel next to Funnel.
$publicUrlFromEnv = $envValues['PUBLIC_URL']
$useQuickTunnel = -not ($envValues['TUNNEL_TOKEN'] -or ($publicUrlFromEnv -match '\.ts\.net$'))
$tunnelArgs = if ($useQuickTunnel) { @('--tunnel') } else { @() }

$portInUse = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if ($portInUse) {
    $owner = (Get-Process -Id $portInUse[0].OwningProcess -ErrorAction SilentlyContinue)
    Fail "Port $Port is already in use$(if ($owner) { " by $($owner.ProcessName) (pid $($owner.Id))" }). Run stop.cmd first, or set a different PORT in .env."
}

# Make sure the resolved port actually reaches the Node process.
$env:PORT = "$Port"

# --- background mode -------------------------------------------------------
if ($Background) {
    Write-Step "Starting relay in the background on port $Port"

    $logDir = Join-Path $root 'logs'
    $runDir = Join-Path $root '.run'
    New-Item -ItemType Directory -Force -Path $logDir, $runDir | Out-Null

    $stdout = Join-Path $logDir 'relay.log'
    $stderr = Join-Path $logDir 'relay.err.log'
    Remove-Item $stdout, $stderr -Force -ErrorAction SilentlyContinue

    $proc = Start-Process -FilePath 'node' -ArgumentList (@('index.js') + $tunnelArgs) `
        -WorkingDirectory $root -RedirectStandardOutput $stdout -RedirectStandardError $stderr `
        -WindowStyle Hidden -PassThru

    Set-Content -Path (Join-Path $runDir 'relay.pid') -Value $proc.Id -Encoding ascii

    # Wait for the tunnel hostname to appear in the log. Named tunnels and
    # Tailscale Funnel print no trycloudflare URL; PUBLIC_URL is used instead.
    $url = $null
    if (-not $useQuickTunnel -and $publicUrlFromEnv) {
        $url = $publicUrlFromEnv.TrimEnd('/')
    }
    for ($i = 0; $i -lt 60 -and -not $url; $i++) {
        Start-Sleep -Milliseconds 700
        if ($proc.HasExited) {
            Fail "The relay exited immediately. See $stderr"
        }
        if (Test-Path $stdout) {
            $text = Get-Content $stdout -Raw
            $m = [regex]::Match($text, 'https://[a-z0-9-]+\.trycloudflare\.com')
            if ($m.Success) { $url = $m.Value; break }
        }
    }

    if (-not $url) {
        Write-Host '    [!] No tunnel URL appeared yet. Check logs\relay.log in a moment.' -ForegroundColor Yellow
        $url = "http://127.0.0.1:$Port"
    }

    $manifest = "$url/manifest.json"
    $manifest | Set-Clipboard
    Write-Host "`nRelay is running (pid $($proc.Id))" -ForegroundColor Green
    Write-Host "  install URL : $manifest"
    Write-Host "  landing page: $url"
    Write-Host "  logs        : $stdout"
    Write-Host "  (install URL copied to clipboard)" -ForegroundColor DarkGray
    if ($Open) { Start-Process $url }
    return
}

# --- foreground mode -------------------------------------------------------
Write-Step "Starting relay on port $Port (Ctrl+C to stop)"
if ($Open) {
    # Give the server a moment, then open the local landing page.
    Start-Job -ScriptBlock {
        param($p)
        for ($i = 0; $i -lt 40; $i++) {
            Start-Sleep -Milliseconds 500
            try {
                $r = Invoke-WebRequest "http://127.0.0.1:$p/health" -TimeoutSec 2 -UseBasicParsing
                if ($r.StatusCode -eq 200) { Start-Process "http://127.0.0.1:$p"; break }
            } catch { }
        }
    } -ArgumentList $Port | Out-Null
}

& node index.js @tunnelArgs
