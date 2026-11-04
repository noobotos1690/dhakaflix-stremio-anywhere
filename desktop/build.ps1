<#
.SYNOPSIS
    Build the DhakaFlix Relay desktop app and its installer.

.DESCRIPTION
    Compiles desktop/src/*.cs against the in-box .NET Framework 4.8
    assemblies with the Roslyn compiler from the .NET SDK, stages the relay
    source next to the exe, draws the app icon and (unless -SkipInstaller is
    used) compiles installer.iss with Inno Setup.

.EXAMPLE
    .\desktop\build.ps1

.EXAMPLE
    .\desktop\build.ps1 -SkipInstaller
#>

[CmdletBinding()]
param(
    [switch]$SkipInstaller,
    [switch]$ForceIcon
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent $here
$srcDir = Join-Path $here 'src'
$assetsDir = Join-Path $here 'assets'
$distDir = Join-Path $here 'dist'
$appDir = Join-Path $distDir 'app'
$iconPath = Join-Path $assetsDir 'app.ico'
$manifest = Join-Path $here 'app.manifest'
$config = Join-Path $here 'app.config'
$version = (Get-Content (Join-Path $repo 'package.json') -Raw | ConvertFrom-Json).version

function Write-Step([string]$message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Write-Ok([string]$message) { Write-Host "    [ok] $message" -ForegroundColor Green }

Write-Step 'Preparing folders'
New-Item -ItemType Directory -Force -Path $assetsDir, $distDir, $appDir | Out-Null
if (Test-Path $appDir) {
    Get-ChildItem $appDir -Force | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}

# --------------------------------------------------------------------- icon
Write-Step 'Preparing the application icon'
$iconScript = Join-Path $here 'make-icon.ps1'
$windowsPowerShell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
if ($ForceIcon -or -not (Test-Path $iconPath)) {
    & $windowsPowerShell -NoProfile -ExecutionPolicy Bypass -File $iconScript -Path $iconPath -Force
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $iconPath)) { throw 'Icon generation failed.' }
    Write-Ok $iconPath
} else {
    Write-Ok "Icon already exists: $iconPath"
}

Write-Step 'Preparing installer artwork'
$wizardBig = Join-Path $assetsDir 'wizard.bmp'
if ($ForceIcon -or -not (Test-Path $wizardBig)) {
    & $windowsPowerShell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $here 'make-art.ps1') -Force
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $wizardBig)) { throw 'Wizard artwork generation failed.' }
    Write-Ok $wizardBig
} else {
    Write-Ok "Artwork already exists: $wizardBig"
}

# ------------------------------------------------------------------ compiler
Write-Step 'Locating the C# compiler'
$dotnet = (Get-Command dotnet -ErrorAction SilentlyContinue).Source
if (-not $dotnet) { throw 'The .NET SDK (dotnet) was not found. Install it from https://dot.net.' }

$sdk = Get-ChildItem 'C:\Program Files\dotnet\sdk' -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdk) { throw 'No .NET SDK folder found under C:\Program Files\dotnet\sdk.' }

$csc = Join-Path $sdk.FullName 'Roslyn\bincore\csc.dll'
if (-not (Test-Path $csc)) { throw "Roslyn compiler not found at $csc" }
Write-Ok "Roslyn $($sdk.Name)"

$refDir = 'C:\Program Files (x86)\Reference Assemblies\Microsoft\Framework\.NETFramework\v4.8'
if (-not (Test-Path $refDir)) { throw ".NET Framework 4.8 reference assemblies not found at $refDir" }
$refs = Get-ChildItem $refDir -Filter *.dll | Where-Object { $_.Name -notlike '*Wrapper.dll' -and $_.Name -notlike '*Thunk.dll' } | ForEach-Object { '/r:' + $_.FullName }
Write-Ok "Reference assemblies: $refDir"

$sources = Get-ChildItem (Join-Path $srcDir '*.cs') | ForEach-Object { $_.FullName }
$exeName = 'DhakaFlixRelay.exe'
$exePath = Join-Path $appDir $exeName

Write-Step "Compiling $exeName"
$arguments = @(
    '/nologo',
    '/target:winexe',
    '/platform:anycpu',
    '/langversion:7.3',
    '/optimize+',
    '/deterministic',
    '/nostdlib+',
    '/utf8output',
    ('/out:' + $exePath),
    ('/win32manifest:' + $manifest),
    ('/win32icon:' + $iconPath)
) + $sources + $refs

& $dotnet $csc @arguments
if ($LASTEXITCODE -ne 0) { throw 'C# compilation failed.' }
Write-Ok $exePath

Copy-Item $config (Join-Path $appDir 'DhakaFlixRelay.exe.config') -Force

# -------------------------------------------------------------------- stage
Write-Step 'Staging relay files next to the app'
$files = @(
    'index.js', 'server.js', 'package.json', 'package-lock.json', 'addon.json',
    '.env.example', 'README.md', 'start.cmd', 'setup.cmd', 'stop.cmd'
)
foreach ($file in $files) {
    $source = Join-Path $repo $file
    if (Test-Path $source) { Copy-Item $source (Join-Path $appDir $file) -Force }
}

foreach ($dir in @('src', 'scripts', 'docs')) {
    $source = Join-Path $repo $dir
    $target = Join-Path $appDir $dir
    if (-not (Test-Path $source)) { continue }
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    Copy-Item (Join-Path $source '*') $target -Recurse -Force
}

# GitHub push helpers do not belong in an installed copy.
Remove-Item (Join-Path $appDir 'scripts\push-to-github.ps1') -Force -ErrorAction SilentlyContinue
foreach ($ignore in @('.env', 'node_modules', 'tools', 'runtime', 'logs', '.run')) {
    $path = Join-Path $appDir $ignore
    if (Test-Path $path) { Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue }
}
$portableReadme = @'
DhakaFlix Relay __VERSION__ - portable build

1. Unzip this folder anywhere (Desktop, USB drive, ...).
2. Run DhakaFlixRelay.exe.
3. The first run installs Node.js, the npm packages, cloudflared and .env
   into this same folder, with progress in the Live output panel.
4. Closing the window keeps the relay running in the notification area.
   Exit from the tray icon when you want to stop it.

No installer and no administrator rights are needed. To uninstall, turn off
"Launch at Windows startup" first, exit from the tray, then delete the folder.
'@
$portableReadme = $portableReadme.Replace('__VERSION__', $version)
[System.IO.File]::WriteAllText((Join-Path $appDir 'README-PORTABLE.txt'), $portableReadme, (New-Object System.Text.UTF8Encoding($false)))
Write-Ok "Staged into $appDir"

# ----------------------------------------------------------------- installed
if (-not $SkipInstaller) {
    Write-Step 'Compiling the installer with Inno Setup'
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
    )
    $iscc = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) {
        Write-Host '    [!] Inno Setup 6 was not found. Install it, or use -SkipInstaller.' -ForegroundColor Yellow
    } else {
        Push-Location $here
        try {
            & $iscc '/Qp' (Join-Path $here 'installer.iss')
            if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }
        } finally {
            Pop-Location
        }
        Write-Ok 'Installer created in desktop\dist'
    }
}

# ------------------------------------------------------------------ portable
Write-Step 'Packaging the portable zip'
$zipPath = Join-Path $distDir ("DhakaFlixRelay-Portable-{0}.zip" -f $version)
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($appDir, $zipPath, [System.IO.Compression.CompressionLevel]::Optimal, $false)
Write-Ok $zipPath

Write-Host "`nBuild complete." -ForegroundColor Green
Write-Host "  App       : $exePath"
if (-not $SkipInstaller -and (Test-Path (Join-Path $distDir ('DhakaFlixRelay-Setup-{0}.exe' -f $version)))) {
    Write-Host "  Installer : $(Join-Path $distDir ('DhakaFlixRelay-Setup-{0}.exe' -f $version))"
}
Write-Host "  Portable  : $zipPath"
