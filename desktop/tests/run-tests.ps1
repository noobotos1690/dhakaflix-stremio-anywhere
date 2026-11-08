<#
Compiles and runs the dependency-free Tailscale/env test harness.

Usage:
    .\desktop\tests\run-tests.ps1
#>

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path (Split-Path -Parent $here) 'src'
$bin = Join-Path $here 'bin'
$exe = Join-Path $bin 'TailscaleTests.exe'
$fixtures = Join-Path $here 'fixtures'

New-Item -ItemType Directory -Force -Path $bin | Out-Null

$dotnet = (Get-Command dotnet -ErrorAction SilentlyContinue).Source
if (-not $dotnet) { throw 'The .NET SDK (dotnet) was not found.' }

$sdk = Get-ChildItem 'C:\Program Files\dotnet\sdk' -Directory | Sort-Object Name -Descending | Select-Object -First 1
$csc = Join-Path $sdk.FullName 'Roslyn\bincore\csc.dll'
$refDir = 'C:\Program Files (x86)\Reference Assemblies\Microsoft\Framework\.NETFramework\v4.8'
$refs = Get-ChildItem $refDir -Filter *.dll |
    Where-Object { $_.Name -notlike '*Wrapper.dll' -and $_.Name -notlike '*Thunk.dll' } |
    ForEach-Object { '/r:' + $_.FullName }

$sources = @(
    (Join-Path $src 'PublicAccess.cs'),
    (Join-Path $src 'EnvFileEditor.cs'),
    (Join-Path $src 'LogEntry.cs'),
    (Join-Path $src 'ProcessRunner.cs'),
    (Join-Path $src 'TailscaleManager.cs'),
    (Join-Path $here 'TailscaleTests.cs')
)

& $dotnet $csc /nologo /target:exe /langversion:7.3 /nostdlib+ ('/out:' + $exe) $sources $refs
if ($LASTEXITCODE -ne 0) { throw 'Test harness failed to compile.' }

& $exe $fixtures
exit $LASTEXITCODE
