<#
.SYNOPSIS
    Stop the DhakaFlix relay (and its tunnel).

.DESCRIPTION
    Finds the relay in three ways, in order:
      1. the pid file written by `start.cmd -Background`
      2. whatever node process is listening on the configured port
      3. an orphaned cloudflared tunnel pointed at that port
    Only processes belonging to this relay are touched.

.PARAMETER Port
    Override the port from .env / the default 7001.

.EXAMPLE
    .\scripts\stop.ps1
#>

[CmdletBinding()]
param(
    [int]$Port = 0
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

if ($Port -le 0) {
    $Port = 7001
    $envFile = Join-Path $root '.env'
    if (Test-Path $envFile) {
        $match = Select-String -Path $envFile -Pattern '^\s*PORT\s*=\s*(\d+)' | Select-Object -First 1
        if ($match) { $Port = [int]$match.Matches[0].Groups[1].Value }
    }
}

$stopped = New-Object System.Collections.ArrayList
$pidFile = Join-Path $root '.run\relay.pid'

function Stop-Tree([int]$targetPid, [string]$label) {
    try {
        & taskkill /PID $targetPid /T /F 2>&1 | Out-Null
        [void]$stopped.Add("$label (pid $targetPid)")
    } catch {
        Write-Host "    could not stop pid $targetPid" -ForegroundColor Yellow
    }
}

# 1. pid file
if (Test-Path $pidFile) {
    $savedPid = (Get-Content $pidFile -Raw).Trim()
    if ($savedPid -match '^\d+$') {
        if (Get-Process -Id ([int]$savedPid) -ErrorAction SilentlyContinue) {
            Stop-Tree ([int]$savedPid) 'relay'
        }
    }
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}

# 2. listener on the port (only if it is node)
if ($stopped.Count -eq 0) {
    $listener = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($listener) {
        $owner = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
        if ($owner -and $owner.ProcessName -eq 'node') {
            Stop-Tree $owner.Id "relay on port $Port"
        } elseif ($owner) {
            Write-Host "Port $Port is held by $($owner.ProcessName), not this relay. Leaving it alone." -ForegroundColor Yellow
        }
    }
}

# 3. orphaned tunnel for this port
$needle = "127.0.0.1:$Port"
$tunnels = Get-CimInstance Win32_Process -Filter "Name = 'cloudflared.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains($needle) }
foreach ($t in $tunnels) {
    if ($stopped -notcontains "tunnel (pid $($t.ProcessId))") {
        Stop-Tree $t.ProcessId 'tunnel'
    }
}

if ($stopped.Count -gt 0) {
    Write-Host "Stopped: $($stopped -join ', ')" -ForegroundColor Green
} else {
    Write-Host 'Nothing was running.' -ForegroundColor Yellow
}
