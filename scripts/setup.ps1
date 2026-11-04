<#
.SYNOPSIS
    First-time setup for the DhakaFlix Stremio relay.

.DESCRIPTION
    Installs npm dependencies, downloads cloudflared and writes a .env with a
    freshly generated PROXY_SECRET. Safe to re-run: existing files are kept.

.EXAMPLE
    .\scripts\setup.ps1
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

function Write-Step($message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Write-Ok($message) { Write-Host "    [ok] $message" -ForegroundColor Green }
function Write-Warn($message) { Write-Host "    [!]  $message" -ForegroundColor Yellow }
function Fail($message) { Write-Host "`n[ERROR] $message" -ForegroundColor Red; exit 1 }

Write-Host "DhakaFlix relay setup" -ForegroundColor Magenta
Write-Host "Project: $root"

# --- 1. Node.js ------------------------------------------------------------
Write-Step 'Checking Node.js'
$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) {
    Fail "Node.js was not found. Install the LTS build from https://nodejs.org/ and run this again."
}

$nodeVersion = (& node --version).TrimStart('v')
$major = [int]($nodeVersion.Split('.')[0])
if ($major -lt 18) {
    Fail "Node.js $nodeVersion is too old. Version 18 or newer is required (LTS recommended)."
}
Write-Ok "Node.js $nodeVersion"

# --- 2. Dependencies -------------------------------------------------------
Write-Step 'Installing npm dependencies'
if (Test-Path (Join-Path $root 'package-lock.json')) {
    & npm ci --omit=dev --no-audit --no-fund
} else {
    & npm install --omit=dev --no-audit --no-fund
}
if ($LASTEXITCODE -ne 0) { Fail 'npm install failed. Check your internet connection and try again.' }
Write-Ok 'Dependencies installed'

# --- 3. cloudflared --------------------------------------------------------
Write-Step 'Checking cloudflared (public tunnel helper)'
$cfNames = @('cloudflared.exe', 'cloudflared')
$cf = $null
foreach ($name in $cfNames) {
    $candidate = Join-Path $root "tools\$name"
    if (Test-Path $candidate) { $cf = $candidate; break }
}
if ($cf) {
    Write-Ok "Found $cf"
} else {
    Write-Host '    Downloading cloudflared...'
    & node (Join-Path $root 'scripts\fetch-cloudflared.js')
    if ($LASTEXITCODE -ne 0) {
        Write-Warn 'cloudflared download failed. Local mode still works; re-run this script to retry the tunnel.'
    } else {
        Write-Ok 'cloudflared downloaded into tools\'
    }
}

# --- 4. .env ---------------------------------------------------------------
Write-Step 'Preparing configuration (.env)'
$envPath = Join-Path $root '.env'
if (Test-Path $envPath) {
    Write-Ok '.env already exists, leaving it untouched'
} else {
    & node -e "const c=require('crypto'),f=require('fs');const secret=c.randomBytes(32).toString('hex');const tpl=f.readFileSync('.env.example','utf8').replace(/^PROXY_SECRET=.*$/m,'PROXY_SECRET='+secret);f.writeFileSync('.env',tpl);console.log('created');"
    if ($LASTEXITCODE -ne 0) { Fail 'Could not create .env' }
    Write-Ok 'Created .env with a generated PROXY_SECRET'
}

# --- 5. Done ---------------------------------------------------------------
Write-Host "`nSetup complete." -ForegroundColor Green
Write-Host @"

Next steps
  start.cmd          start the relay and get a public install URL
  start.cmd -Open    same, and open the install page in your browser
  stop.cmd           stop it again

The public URL changes every restart with the free quick tunnel. For a URL that
stays the same, see "Stable hostname" in README.md.

"@
