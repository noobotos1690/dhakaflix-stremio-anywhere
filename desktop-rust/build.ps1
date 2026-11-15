<#
.SYNOPSIS
    Build the Rust + Slint DhakaFlix Relay dashboard, its installer and portable zip.

.DESCRIPTION
    Runs cargo build --release, stages the relay source and its bundled
    dependencies next to the exe, writes the portable readme, compiles
    installer.iss and portable.iss with Inno Setup when it is installed, and
    zips the staged folder.

.EXAMPLE
    .\desktop-rust\build.ps1

.EXAMPLE
    .\desktop-rust\build.ps1 -SkipInstaller
#>

[CmdletBinding()]
param(
    [switch]$SkipInstaller
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent $here
$distDir = Join-Path $here 'dist'
$appDir = Join-Path $distDir 'app'
$exePath = Join-Path $here 'target\release\DhakaFlixRelay.exe'
# The desktop app has its own version, independent of the relay package.
$manifest = Get-Content (Join-Path $here 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'No version found in Cargo.toml.' }

foreach ($issName in @('installer.iss', 'portable.iss')) {
    $iss = Get-Content (Join-Path $here $issName) -Raw
    $issVersion = [regex]::Match($iss, '#define AppVersion "([^"]+)"').Groups[1].Value
    if ($issVersion -ne $version) {
        throw "$issName says $issVersion but Cargo.toml says $version. Keep them in sync."
    }
}

function Write-Step([string]$message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Write-Ok([string]$message) { Write-Host "    [ok] $message" -ForegroundColor Green }

Write-Step 'Compiling the Rust application'
Push-Location $here
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
if (-not (Test-Path $exePath)) { throw "Expected $exePath after the build." }
Write-Ok $exePath

Write-Step 'Staging the relay next to the app'
New-Item -ItemType Directory -Force -Path $distDir, $appDir | Out-Null
if (Test-Path $appDir) {
    Get-ChildItem $appDir -Force | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}
Copy-Item $exePath (Join-Path $appDir 'DhakaFlixRelay.exe') -Force

$files = @(
    'index.js', 'server.js', 'package.json', 'package-lock.json', 'addon.json',
    '.env.example', 'start.cmd', 'setup.cmd', 'stop.cmd'
)
foreach ($file in $files) {
    $source = Join-Path $repo $file
    if (Test-Path $source) { Copy-Item $source (Join-Path $appDir $file) -Force }
}

foreach ($dir in @('src', 'scripts', 'docs')) {
    $source = Join-Path $repo $dir
    if (-not (Test-Path $source)) { continue }
    $target = Join-Path $appDir $dir
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    Copy-Item (Join-Path $source '*') $target -Recurse -Force
}

# GitHub push helpers and per-machine state do not belong in a shipped copy.
Remove-Item (Join-Path $appDir 'scripts\push-to-github.ps1') -Force -ErrorAction SilentlyContinue
foreach ($skip in @('.env', 'node_modules', 'tools', 'runtime', 'logs', '.run', 'target')) {
    $path = Join-Path $appDir $skip
    if (Test-Path $path) { Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue }
}

function Copy-Tree([string]$source, [string]$destination) {
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    & robocopy $source $destination /E /NFL /NDL /NJH /NJS /NP /R:1 /W:1 | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "Copying $source failed (robocopy exit $LASTEXITCODE)." }
}

Write-Step 'Bundling Node.js, npm packages and cloudflared'

# Node.js runtime. The app prefers runtime\node\node.exe over anything on PATH,
# so shipping a copy makes the folder independent of a system-wide install.
$nodeExe = @(
    (Join-Path $env:ProgramFiles 'nodejs\node.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\nodejs\node.exe'),
    (Join-Path $env:LOCALAPPDATA 'nodejs\node.exe')
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $nodeExe) {
    $onPath = Get-Command node.exe -ErrorAction SilentlyContinue
    if ($onPath) { $nodeExe = $onPath.Source }
}
if (-not $nodeExe) {
    throw 'Node.js 18+ must be installed on this machine to bundle the runtime. Install it, then run this script again.'
}

$nodeVersion = ((& $nodeExe --version) -replace '^v', '').Trim()
if ([int]($nodeVersion.Split('.')[0]) -lt 18) {
    throw "Node.js $nodeVersion is too old; the relay needs 18 or newer."
}

$nodeSource = Split-Path -Parent $nodeExe
$nodeTarget = Join-Path $appDir 'runtime\node'
New-Item -ItemType Directory -Force -Path $nodeTarget | Out-Null
Copy-Item $nodeExe (Join-Path $nodeTarget 'node.exe') -Force
# npm itself is only used by Repair dependencies; the packages ship below.
foreach ($item in @('node_modules\npm', 'npm.cmd', 'npx.cmd', 'nodevars.bat')) {
    $source = Join-Path $nodeSource $item
    if (-not (Test-Path $source)) { continue }
    if ((Get-Item $source) -is [System.IO.DirectoryInfo]) {
        Copy-Tree $source (Join-Path $nodeTarget $item)
    } else {
        Copy-Item $source (Join-Path $nodeTarget $item) -Force
    }
}
Write-Ok "Node.js $nodeVersion in runtime\node"

# The relay's npm packages, so the first start does not need npm install.
$modulesSource = Join-Path $repo 'node_modules'
if (-not (Test-Path (Join-Path $modulesSource 'stremio-addon-sdk\package.json'))) {
    throw 'node_modules is missing here. Run npm install in the repository, then package again.'
}
Copy-Tree $modulesSource (Join-Path $appDir 'node_modules')
Write-Ok 'node_modules'

# cloudflared, used by the Cloudflare quick tunnel mode.
$cloudflaredTarget = Join-Path $appDir 'tools\cloudflared.exe'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $cloudflaredTarget) | Out-Null
$cloudflaredSource = @(
    (Join-Path $repo 'tools\cloudflared.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\DhakaFlix Relay\tools\cloudflared.exe')
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($cloudflaredSource) {
    Copy-Item $cloudflaredSource $cloudflaredTarget -Force
} else {
    Write-Host '    [...] no cached copy, downloading cloudflared' -ForegroundColor Yellow
    Push-Location $appDir
    try {
        & $nodeExe (Join-Path $appDir 'scripts\fetch-cloudflared.js')
        if ($LASTEXITCODE -ne 0) { throw 'Downloading cloudflared failed.' }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path $cloudflaredTarget)) { throw 'cloudflared could not be bundled.' }
Write-Ok 'cloudflared'

$payload = (Get-ChildItem $appDir -Recurse -File -Force | Measure-Object Length -Sum)
Write-Ok ("{0:N1} MB in {1} files" -f ($payload.Sum / 1MB), $payload.Count)

$portableReadme = @'
DhakaFlix Relay __VERSION__ - portable build (Rust + Slint)

1. Unpack this folder anywhere: Desktop, USB drive, another PC.
2. Run DhakaFlixRelay.exe.

Everything the relay needs is inside this folder:

  DhakaFlixRelay.exe    the dashboard
  runtime\node          a private Node.js runtime (used instead of any
                        system-wide install)
  node_modules          the relay's npm packages, already installed
  tools\cloudflared.exe the Cloudflare quick tunnel client
  src, index.js         the relay itself

Nothing is downloaded on first start. Logs and settings go to
%APPDATA%\DhakaFlix Relay.

The only file the app adds here is .env: the first start writes it with a
new random PROXY_SECRET, so stream links keep working after a restart.
Delete that file to make the app generate a new one.

No installer and no administrator rights are needed.

Closing the window keeps the relay running in the notification area. Use the
tray icon to bring the window back or to exit.

To uninstall, turn off "Launch at Windows startup" first, exit from the tray,
then delete the folder.
'@
$portableReadme = $portableReadme.Replace('__VERSION__', $version)
[System.IO.File]::WriteAllText((Join-Path $appDir 'README-PORTABLE.txt'), $portableReadme, (New-Object System.Text.UTF8Encoding($false)))
Write-Ok "Staged into $appDir"

if (-not $SkipInstaller) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 7\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
    )
    $iscc = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) {
        Write-Host '    [!] Inno Setup (7 or 6) was not found. Install it, or use -SkipInstaller.' -ForegroundColor Yellow
    } else {
        foreach ($script in @('installer.iss', 'portable.iss')) {
            $label = if ($script -eq 'installer.iss') { 'installer' } else { 'single-file portable exe' }
            Write-Step "Compiling the $label with Inno Setup"
            Push-Location $here
            try {
                & $iscc '/Qp' (Join-Path $here $script)
                if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed on $script with exit code $LASTEXITCODE" }
            } finally {
                Pop-Location
            }
            Write-Ok "$label created in desktop-rust\dist"
        }
    }
}

Write-Step 'Packaging the portable zip'
$zipPath = Join-Path $distDir ("DhakaFlixRelay-Portable-{0}.zip" -f $version)
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($appDir, $zipPath, [System.IO.Compression.CompressionLevel]::Optimal, $false)
Write-Ok $zipPath

$portableExePath = Join-Path $distDir ('DhakaFlixRelay-Portable-{0}.exe' -f $version)

Write-Host "`nBuild complete." -ForegroundColor Green
Write-Host ("  Payload   : {0:N1} MB in {1} files" -f ($payload.Sum / 1MB), $payload.Count)
Write-Host "  App       : $exePath"
$setup = Join-Path $distDir ('DhakaFlixRelay-Setup-{0}.exe' -f $version)
if (Test-Path $setup) { Write-Host "  Installer : $setup" }
if (Test-Path $portableExePath) { Write-Host "  Portable  : $portableExePath" }
Write-Host "  Zip       : $zipPath"
