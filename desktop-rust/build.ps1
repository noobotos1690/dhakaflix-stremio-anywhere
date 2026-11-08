param(
    [switch]$SkipInstaller,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$version = '2.0.0'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$project = Split-Path -Parent $root
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist 'app'

if (-not $SkipBuild) {
    Push-Location $root
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed: $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage -Force | Out-Null

Copy-Item (Join-Path $root 'target\release\DhakaFlixRelay.exe') $stage
Copy-Item (Join-Path $root 'assets\app.ico') $stage
Copy-Item (Join-Path $root 'README.md') $stage
Copy-Item (Join-Path $root 'THIRD_PARTY_NOTICES.md') $stage
Copy-Item (Join-Path $root 'MIGRATION.md') $stage

foreach ($file in @('index.js', 'server.js', 'addon.json', 'package.json', 'package-lock.json', '.env.example')) {
    Copy-Item (Join-Path $project $file) $stage
}
foreach ($dir in @('src', 'scripts')) {
    Copy-Item (Join-Path $project $dir) $stage -Recurse
}

$zip = Join-Path $dist "DhakaFlixRelay-Portable-$version.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal

if (-not $SkipInstaller) {
    $iscc = Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
    if (-not (Test-Path $iscc)) { $iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
    if (-not (Test-Path $iscc)) { throw 'Inno Setup 6 (ISCC.exe) was not found.' }
    Push-Location $root
    try {
        & $iscc 'installer.iss'
        if ($LASTEXITCODE -ne 0) { throw "ISCC failed: $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

Get-ChildItem $dist -File | Where-Object { $_.Extension -in '.exe', '.zip' } |
    Select-Object Name, Length, @{Name = 'SHA256'; Expression = { (Get-FileHash $_.FullName -Algorithm SHA256).Hash } } |
    Format-Table -AutoSize
