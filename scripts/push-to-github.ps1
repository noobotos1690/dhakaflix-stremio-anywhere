<#
.SYNOPSIS
    Commit your work and push it to your own GitHub fork.

.DESCRIPTION
    This repository is a fork of Niloy-Sarker/dhakaflix-stremio-addon. This script
    sets up the conventional remote layout:

        origin   -> https://github.com/<you>/<repo>.git   (your fork)
        upstream -> https://github.com/Niloy-Sarker/dhakaflix-stremio-addon.git

    It then commits any pending changes and pushes. If GitHub credentials are not
    cached yet, Git Credential Manager opens a browser window once - sign in and
    the push completes by itself.

.PARAMETER User
    Your GitHub username. Defaults to the git user.name in this repo.

.PARAMETER Repo
    Repository name to push to. Default: dhakaflix-stremio-addon

.PARAMETER Message
    Commit message for pending changes.

.PARAMETER Force
    Allow a non-fast-forward push (--force-with-lease) of your branch.

.EXAMPLE
    .\scripts\push-to-github.ps1

.EXAMPLE
    .\scripts\push-to-github.ps1 -User myname -Repo my-addon
#>

[CmdletBinding()]
param(
    [string]$User = '',
    [string]$Repo = 'dhakaflix-stremio-addon',
    [string]$Message = '',
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

$upstreamSlug = 'Niloy-Sarker/dhakaflix-stremio-addon'
$upstreamUrl = "https://github.com/$upstreamSlug.git"

function Info($m) { Write-Host $m }
function Step($m) { Write-Host "`n==> $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "    [ok] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "    [!]  $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "`n[ERROR] $m" -ForegroundColor Red; exit 1 }

if (-not (Test-Path (Join-Path $root '.git'))) { Fail 'Not a git repository.' }

if (-not $User) {
    $User = (& git config user.name).Trim()
}
if (-not $User) { Fail 'Could not determine your GitHub username. Pass -User yourname.' }

$branch = (& git branch --show-current).Trim()
if (-not $branch) { $branch = 'main' }
$slug = "$User/$Repo"
$forkUrl = "https://github.com/$slug.git"

Info "Repository : $slug"
Info "Branch     : $branch"
Info "Upstream   : $upstreamSlug"

# --- 1. Does the remote repo exist? ---------------------------------------
Step 'Checking that your repository exists on GitHub'
$exists = $false
try {
    $headers = @{ 'User-Agent' = 'dhakaflix-relay'; 'Accept' = 'application/vnd.github+json' }
    $null = Invoke-RestMethod -Uri "https://api.github.com/repos/$slug" -Headers $headers -TimeoutSec 20
    $exists = $true
    Ok "$slug exists"
} catch {
    $status = $null
    try { $status = $_.Exception.Response.StatusCode.value__ } catch { }
    if ($status -eq 404) {
        Warn "$slug was not found (or is private)."
    } else {
        Warn "Could not verify $slug online; continuing anyway."
    }
}

if (-not $exists) {
    Write-Host @"

Create it first (one of these), then run this script again:

  A) Recommended - make it a real fork so GitHub shows the lineage:
     https://github.com/$upstreamSlug/fork

  B) Or an empty repository with the same name:
     https://github.com/new   (name: $Repo, do not add a README)

"@ -ForegroundColor Yellow
    exit 1
}

# --- 2. Remotes ------------------------------------------------------------
Step 'Configuring remotes'
$remotes = @{}
foreach ($line in (& git remote -v)) {
    $parts = $line -split '\s+'
    if ($parts.Count -ge 2) { $remotes[$parts[0]] = $parts[1] }
}

$originUrl = $remotes['origin']
if ($originUrl -and $originUrl -match 'Niloy-Sarker/dhakaflix-stremio-addon') {
    Info 'origin currently points at the upstream project; renaming it to "upstream".'
    & git remote rename origin upstream | Out-Null
    & git remote add origin $forkUrl
    Ok "origin -> $forkUrl"
} elseif (-not $remotes['origin']) {
    & git remote add origin $forkUrl
    Ok "origin -> $forkUrl"
} elseif ($remotes['origin'] -ne $forkUrl) {
    Info "origin points at $($remotes['origin']); setting it to your fork."
    & git remote set-url origin $forkUrl
    Ok "origin -> $forkUrl"
} else {
    Ok "origin -> $forkUrl"
}

$remotes = @{}
foreach ($line in (& git remote -v)) {
    $parts = $line -split '\s+'
    if ($parts.Count -ge 2) { $remotes[$parts[0]] = $parts[1] }
}
if (-not $remotes['upstream']) {
    & git remote add upstream $upstreamUrl 2>&1 | Out-Null
    Ok "upstream -> $upstreamUrl"
}

# --- 3. Commit pending work -----------------------------------------------
Step 'Committing pending changes'
$pending = (& git status --porcelain)
if ($pending) {
    if (-not $Message) {
        $Message = 'Add public relay, setup scripts and documentation'
    }
    & git add -A
    & git commit -m $Message | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'git commit failed.' }
    Ok "Committed: $Message"
} else {
    Ok 'Working tree already clean'
}

# Safety: never commit secrets
$tracked = & git ls-files
foreach ($sensitive in @('.env', 'tools/cloudflared.exe')) {
    if ($tracked -contains $sensitive) {
        Fail "$sensitive is tracked by git. Remove it with: git rm --cached $sensitive"
    }
}

# --- 4. Push ---------------------------------------------------------------
Step "Pushing $branch to $slug"
& git push -u origin $branch
if ($LASTEXITCODE -ne 0) {
    if ($Force) {
        Warn 'Normal push failed, retrying with --force-with-lease'
        & git push --force-with-lease -u origin $branch
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host @"

Push failed. Most common causes:

  * You were not signed in to GitHub yet - Git Credential Manager should have
    opened a browser window. Sign in, then run this script again.
  * The remote has commits you do not have locally. Run:
        git pull --rebase origin $branch
    then run this script again (or use -Force if you are sure).

"@ -ForegroundColor Yellow
        exit 1
    }
}

Ok 'Pushed successfully'
Write-Host @"

Repository : https://github.com/$slug
Install    : https://github.com/$slug#quick-start

Anyone can now clone it with:
  git clone $forkUrl

"@ -ForegroundColor Green
