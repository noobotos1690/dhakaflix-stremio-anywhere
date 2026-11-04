<#
.SYNOPSIS
    Start the relay automatically when you log in to Windows.

.DESCRIPTION
    Registers (or removes) a per-user Scheduled Task named "DhakaFlix Relay".
    No administrator rights are needed and nothing is installed system-wide.

.PARAMETER Install
    Create/refresh the logon task.

.PARAMETER Uninstall
    Remove the logon task.

.PARAMETER Status
    Show whether the task exists and when it last ran.

.EXAMPLE
    .\scripts\autostart.ps1 -Install

.EXAMPLE
    .\scripts\autostart.ps1 -Status
#>

[CmdletBinding()]
param(
    [switch]$Install,
    [switch]$Uninstall,
    [switch]$Status
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$taskName = 'DhakaFlix Relay'
$startScript = Join-Path $root 'scripts\start.ps1'

if (-not ($Install -or $Uninstall -or $Status)) {
    Write-Host 'Usage: .\scripts\autostart.ps1 -Install | -Uninstall | -Status' -ForegroundColor Yellow
    exit 0
}

if ($Install) {
    if (-not (Test-Path $startScript)) { Write-Host "Missing $startScript" -ForegroundColor Red; exit 1 }

    $action = New-ScheduledTaskAction -Execute 'powershell.exe' `
        -Argument "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$startScript`" -Background" `
        -WorkingDirectory $root
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -StartWhenAvailable `
        -RestartCount 3 `
        -RestartInterval (New-TimeSpan -Minutes 2) `
        -ExecutionTimeLimit (New-TimeSpan -Days 0)

    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings `
        -Description 'Starts the DhakaFlix Stremio relay at logon so the addon keeps working.' -Force | Out-Null

    Write-Host "Installed scheduled task '$taskName'." -ForegroundColor Green
    Write-Host 'The relay will start automatically the next time you log in.'
    Write-Host 'Start it now with: start.cmd'
    exit 0
}

if ($Uninstall) {
    $existing = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if (-not $existing) {
        Write-Host "Task '$taskName' is not installed." -ForegroundColor Yellow
        exit 0
    }
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    Write-Host "Removed scheduled task '$taskName'." -ForegroundColor Green
    exit 0
}

if ($Status) {
    $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if (-not $task) {
        Write-Host "Task '$taskName' is not installed." -ForegroundColor Yellow
        exit 0
    }
    $info = Get-ScheduledTaskInfo -TaskName $taskName
    Write-Host "Task '$taskName' is installed." -ForegroundColor Green
    Write-Host "  state      : $($task.State)"
    Write-Host "  last run   : $($info.LastRunTime)"
    Write-Host "  last result: $($info.LastTaskResult)"
    exit 0
}
