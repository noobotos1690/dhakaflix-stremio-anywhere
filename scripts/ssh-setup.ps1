<#
.SYNOPSIS
    Set up or inspect Windows OpenSSH Server for private remote administration.

.DESCRIPTION
    This script prepares THIS Windows machine to accept SSH connections. It is
    designed to be used together with Tailscale: the firewall rule it manages is
    restricted to the Tailscale address range, so port 22 is never exposed to
    the public internet or the LAN.

    It never overwrites sshd_config, never deletes existing authorized keys,
    never disables password authentication, and never creates an account.

    Modes:
      -Status    (default) report what is installed and running. No admin needed.
      -Install   install OpenSSH Server, start sshd, create a Tailscale-only
                 firewall rule. Requires an elevated PowerShell.
      -AddKey    append a client public key to authorized_keys with correct
                 permissions. Requires an elevated PowerShell.
      -Uninstall stop/disable sshd and remove the firewall rule this script
                 created. Requires an elevated PowerShell.

.EXAMPLE
    .\scripts\ssh-setup.ps1 -Status

.EXAMPLE
    # Elevated PowerShell:
    .\scripts\ssh-setup.ps1 -Install
    .\scripts\ssh-setup.ps1 -AddKey "ssh-ed25519 AAAAC3... user@laptop"
#>

[CmdletBinding()]
param(
    [switch]$Status,
    [switch]$Install,
    [switch]$Uninstall,
    [switch]$AddKey,
    [string]$PublicKey = '',
    # Restrict the stock "OpenSSH-Server-In-TCP" firewall rule (which allows
    # every remote address) down to the Tailscale ranges. Off by default so an
    # existing LAN SSH setup is never broken silently.
    [switch]$RestrictToTailscale
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

# Tailscale assigns IPv4 from 100.64.0.0/10 (CGNAT) and IPv6 from
# fd7a:115c:a1e0::/48. SSH should only ever be reachable from those ranges.
$tailscaleV4 = '100.64.0.0/10'
$tailscaleV6 = 'fd7a:115c:a1e0::/48'
$scopedRuleName = 'OpenSSH-Tailscale-Only'
$stockRuleName = 'OpenSSH-Server-In-TCP'

function Write-Step($message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Write-Ok($message) { Write-Host "    [ok] $message" -ForegroundColor Green }
function Write-Warn($message) { Write-Host "    [!]  $message" -ForegroundColor Yellow }
function Write-Info($message) { Write-Host "    $message" -ForegroundColor DarkGray }
function Fail($message) { Write-Host "`n[ERROR] $message" -ForegroundColor Red; exit 1 }

function Test-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($id)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# sshd chooses administrators_authorized_keys based on ACCOUNT membership in
# Administrators, which is unrelated to whether this script runs elevated.
function Test-AccountAdministrator {
    try {
        $group = Get-LocalGroup -SID 'S-1-5-32-544' -ErrorAction Stop   # BUILTIN\Administrators
        $mySid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $members = Get-LocalGroupMember -Group $group.Name -ErrorAction Stop
        return [bool]($members | Where-Object { $_.SID.Value -eq $mySid })
    } catch {
        return $false
    }
}

function Get-SshdState {
    $sshdExe = Join-Path $env:SystemRoot 'System32\OpenSSH\sshd.exe'
    $installed = Test-Path $sshdExe
    $service = Get-Service -Name sshd -ErrorAction SilentlyContinue
    [pscustomobject]@{
        Installed  = $installed
        Service    = if ($service) { $service.Status.ToString() } else { 'not registered' }
        Startup    = if ($service) { $service.StartType.ToString() } else { '-' }
    }
}

function Get-TailscaleInfo {
    $exe = Get-Command tailscale -ErrorAction SilentlyContinue
    if (-not $exe) { return $null }
    try {
        $json = & tailscale status --json 2>$null | ConvertFrom-Json
        $ip = ($json.Self.TailscaleIPs | Where-Object { $_ -match '^100\.' } | Select-Object -First 1)
        [pscustomobject]@{
            Connected = ($json.BackendState -eq 'Running')
            IPv4      = $ip
            Hostname  = ($json.Self.DNSName -replace '\.$', '')
        }
    } catch {
        return $null
    }
}

function Show-Status {
    Write-Step 'OpenSSH Server status (this machine)'

    $client = Get-Command ssh -ErrorAction SilentlyContinue
    Write-Info ("SSH client : " + $(if ($client) { $client.Source } else { 'not found (install "OpenSSH Client" optional feature)' }))

    $sshd = Get-SshdState
    if ($sshd.Installed) {
        Write-Ok "OpenSSH Server installed (sshd: $($sshd.Service), startup: $($sshd.Startup))"
    } else {
        Write-Warn 'OpenSSH Server is NOT installed. Run this script with -Install from an elevated PowerShell.'
    }

    Write-Step 'Firewall rules for port 22'
    $rules = Get-NetFirewallRule -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -match 'OpenSSH|SSH' -or $_.Name -match 'OpenSSH' }
    if (-not $rules) {
        Write-Warn 'No SSH firewall rules found.'
    } else {
        foreach ($rule in $rules) {
            $addr = ($rule | Get-NetFirewallAddressFilter -ErrorAction SilentlyContinue).RemoteAddress
            Write-Info ("{0} [{1}] enabled={2} remote={3}" -f $rule.Name, $rule.DisplayName, $rule.Enabled, ($addr -join ','))
        }
        $scoped = $rules | Where-Object Name -eq $scopedRuleName
        $stock = $rules | Where-Object Name -eq $stockRuleName
        if ($stock -and -not $scoped) {
            Write-Warn "'$stockRuleName' allows SSH from ANY address. Re-run -Install -RestrictToTailscale (elevated) to limit it to Tailscale only, or remove it and keep only '$scopedRuleName'."
        }
    }

    Write-Step 'Tailscale (private network for SSH)'
    $ts = Get-TailscaleInfo
    if (-not $ts) {
        Write-Warn 'tailscale.exe not found on PATH. Install Tailscale and sign in first.'
    } elseif (-not $ts.Connected) {
        Write-Warn 'Tailscale is installed but not connected. Open the Tailscale app and sign in.'
    } else {
        Write-Ok "Connected as $($ts.Hostname) ($($ts.IPv4))"
        Write-Info 'Connect with: ssh <your-user>@' + $ts.Hostname
    }

    Write-Step 'authorized_keys'
    if (Test-AccountAdministrator) {
        $keyFile = 'C:\ProgramData\ssh\administrators_authorized_keys'
        Write-Info "Your account is an administrator -> keys go in $keyFile"
    } else {
        $keyFile = Join-Path $env:USERPROFILE '.ssh\authorized_keys'
        Write-Info "Standard user -> keys go in $keyFile"
    }
    if (Test-Path $keyFile) {
        $count = @(Get-Content $keyFile | Where-Object { $_ -match '^\s*ssh-' }).Count
        Write-Ok "$keyFile exists with $count key(s)"
    } else {
        Write-Info "$keyFile does not exist yet. Add a key with -AddKey."
    }
}

function Install-Sshd {
    if (-not (Test-Admin)) { Fail '-Install requires an elevated PowerShell (Run as administrator).' }

    Write-Step 'Installing OpenSSH Server capability'
    $cap = Get-WindowsCapability -Online -Name 'OpenSSH.Server*' -ErrorAction SilentlyContinue
    if (-not $cap) {
        Fail 'Cannot query Windows capabilities. Is this a supported Windows 10/11 or Server edition?'
    }
    if ($cap.State -eq 'Installed') {
        Write-Ok 'OpenSSH Server capability is already installed'
    } else {
        Write-Info "State: $($cap.State) - installing (needs internet, may take a minute)..."
        $result = Add-WindowsCapability -Online -Name $cap.Name
        if ($result.RestartNeeded) { Write-Warn 'Windows reports a restart is needed.' }
        Write-Ok 'OpenSSH Server installed'
    }

    Write-Step 'Configuring the sshd service'
    Set-Service -Name sshd -StartupType Automatic
    $svc = Get-Service sshd
    if ($svc.Status -ne 'Running') { Start-Service sshd }
    Write-Ok "sshd is $((Get-Service sshd).Status.ToString().ToLower()) and set to start automatically"

    Write-Step 'Firewall: Tailscale-only rule for port 22'
    $existing = Get-NetFirewallRule -Name $scopedRuleName -ErrorAction SilentlyContinue
    if ($existing) {
        Write-Ok "Rule '$scopedRuleName' already exists"
    } else {
        New-NetFirewallRule -Name $scopedRuleName `
            -DisplayName 'OpenSSH Server (Tailscale only)' `
            -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 `
            -RemoteAddress @($tailscaleV4, $tailscaleV6) | Out-Null
        Write-Ok "Created '$scopedRuleName' allowing TCP 22 only from $tailscaleV4 and $tailscaleV6"
    }

    $stock = Get-NetFirewallRule -Name $stockRuleName -ErrorAction SilentlyContinue
    if ($stock -and $stock.Enabled -eq 'True') {
        $addr = ($stock | Get-NetFirewallAddressFilter).RemoteAddress
        $open = ($addr -contains 'Any' -or $addr -contains '0.0.0.0/0')
        if ($open) {
            Write-Warn "The stock rule '$stockRuleName' still allows SSH from ANY address, which defeats the Tailscale-only restriction."
            if ($RestrictToTailscale) {
                Set-NetFirewallRule -Name $stockRuleName -RemoteAddress @($tailscaleV4, $tailscaleV6)
                Write-Ok "Restricted '$stockRuleName' to the Tailscale ranges"
            } else {
                Write-Info "Re-run with -RestrictToTailscale to limit it, or remove it: Remove-NetFirewallRule -Name '$stockRuleName'"
            }
        } else {
            Write-Ok "Stock rule '$stockRuleName' is already restricted"
        }
    }

    Write-Step 'Next steps'
    Write-Host @"
  1. On your CLIENT device, generate a key (see docs/WINDOWS-SSH-SETUP.md):
       ssh-keygen -t ed25519
  2. Add the public key here:
       .\scripts\ssh-setup.ps1 -AddKey "ssh-ed25519 AAAA... you@device"
  3. Connect through Tailscale (never the public IP):
       ssh $env:USERNAME@<tailscale-hostname-or-100.x.y.z>

  Password login still works until you disable it in
  C:\ProgramData\ssh\sshd_config - test key login FIRST.
"@
}

function Add-AuthorizedKey {
    if (-not (Test-Admin)) { Fail '-AddKey requires an elevated PowerShell (Run as administrator).' }
    if ([string]::IsNullOrWhiteSpace($PublicKey)) { Fail '-AddKey needs the public key string, e.g. -AddKey "ssh-ed25519 AAAA... comment"' }
    if ($PublicKey -notmatch '^(ssh-|ecdsa-|sk-)[A-Za-z0-9+/=]+') { Fail 'That does not look like an SSH public key.' }

    if (Test-AccountAdministrator) {
        $keyFile = 'C:\ProgramData\ssh\administrators_authorized_keys'
    } else {
        $keyFile = Join-Path $env:USERPROFILE '.ssh\authorized_keys'
        New-Item -ItemType Directory -Force -Path (Split-Path $keyFile) | Out-Null
    }

    $existing = if (Test-Path $keyFile) { Get-Content $keyFile -Raw } else { '' }
    if ($existing -match [regex]::Escape(($PublicKey -split '\s+')[1])) {
        Write-Ok "Key already present in $keyFile - nothing to do"
        return
    }
    Add-Content -Path $keyFile -Value $PublicKey
    Write-Ok "Appended key to $keyFile"

    # administrators_authorized_keys: SYSTEM + Administrators only.
    # Per-user authorized_keys: the owning user + SYSTEM only.
    if (Test-AccountAdministrator) {
        & icacls.exe $keyFile /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F' | Out-Null
    } else {
        & icacls.exe $keyFile /inheritance:r /grant "$($env:USERNAME):F" /grant 'SYSTEM:F' | Out-Null
    }
    if ($LASTEXITCODE -eq 0) { Write-Ok 'Permissions set correctly' }
    else { Write-Warn 'icacls reported an error; check permissions on the key file manually.' }
}

function Uninstall-Sshd {
    if (-not (Test-Admin)) { Fail '-Uninstall requires an elevated PowerShell (Run as administrator).' }

    $svc = Get-Service sshd -ErrorAction SilentlyContinue
    if ($svc) {
        if ($svc.Status -eq 'Running') { Stop-Service sshd }
        Set-Service sshd -StartupType Disabled
        Write-Ok 'sshd stopped and set to Disabled (the OpenSSH capability itself is left installed)'
    } else {
        Write-Info 'sshd service not found; nothing to stop'
    }

    foreach ($name in @($scopedRuleName)) {
        if (Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue) {
            Remove-NetFirewallRule -Name $name
            Write-Ok "Removed firewall rule '$name'"
        }
    }
    Write-Info "Stock rule '$stockRuleName' (if present) was not created by this script and was left alone."
    Write-Info 'To remove OpenSSH entirely: Remove-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0'
}

if ($Install) { Install-Sshd; exit 0 }
if ($Uninstall) { Uninstall-Sshd; exit 0 }
if ($AddKey) { Add-AuthorizedKey; exit 0 }
Show-Status
