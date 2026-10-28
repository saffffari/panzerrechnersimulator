<#
.SYNOPSIS
  Allow Panzerrechnersystem LAN crew co-op through Windows Defender Firewall (Private networks).

.DESCRIPTION
  Adds exactly two inbound rules, both limited to the game's executable and to the Private
  network profile (a home Wi-Fi you marked Private; never Public/guest networks):

    Panzerrechnersystem LAN game (UDP 28460)       the host's gameplay port
    Panzerrechnersystem LAN discovery (UDP 28461)  the "nearby Tigers" browse list

  Nothing else is changed: no other rule, profile, network category or firewall setting. Running it
  again replaces the same two rules (for example after moving the game folder). Run it on the
  laptop that HOSTS; clients need it only for discovery (joining by IP works without it).

  Changing firewall rules needs administrator rights, so the script asks Windows to re-run itself
  elevated (a UAC prompt) when it is not already.

  If you once answered the "Windows Security Alert" prompt with Cancel, Windows added BLOCK rules
  for the executable, and a block wins over an allow. -RemoveBlockRules deletes only the inbound
  block rules whose program is this executable.

.PARAMETER Exe
  The game executable. Default: panzerrechnersystem.exe beside this script's folder (the portable
  package), else <CARGO_TARGET_DIR or target>\release\panzerrechnersystem.exe of the checkout.

.PARAMETER RemoveBlockRules
  Also delete inbound BLOCK rules for this executable (left by a Cancel at the Windows prompt).

.PARAMETER Remove
  Delete the two Panzerrechnersystem rules instead of adding them.

.PARAMETER Check
  Report the rules, block rules and network category; change nothing and do not elevate.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\setup_firewall.ps1
  powershell -ExecutionPolicy Bypass -File tools\setup_firewall.ps1 -Check
  powershell -ExecutionPolicy Bypass -File tools\setup_firewall.ps1 -RemoveBlockRules
  powershell -ExecutionPolicy Bypass -File tools\setup_firewall.ps1 -Remove
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [switch]$RemoveBlockRules,
    [switch]$Remove,
    [switch]$Check
)
$ErrorActionPreference = 'Stop'
$GamePort = 28460
$DiscoveryPort = 28461
$Group = 'Panzerrechnersystem LAN'
$Rules = @(
    @{ Name = 'Panzerrechnersystem-LAN-Game-UDP-28460'; Display = "Panzerrechnersystem LAN game (UDP $GamePort)"; Port = $GamePort },
    @{ Name = 'Panzerrechnersystem-LAN-Discovery-UDP-28461'; Display = "Panzerrechnersystem LAN discovery (UDP $DiscoveryPort)"; Port = $DiscoveryPort }
)

if (-not $Exe) {
    $packaged = Join-Path (Split-Path $PSScriptRoot) 'panzerrechnersystem.exe'
    $beside = Join-Path $PSScriptRoot 'panzerrechnersystem.exe'
    $root = Split-Path $PSScriptRoot
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
    $built = Join-Path $target 'release\panzerrechnersystem.exe'
    $Exe = @($beside, $packaged, $built) | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
    if (-not $Exe) { throw "panzerrechnersystem.exe not found beside the script, in the package folder or at $built; pass -Exe <path>" }
}
$Exe = (Resolve-Path $Exe).Path

function Get-GameBlockRules([string]$program) {
    Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue |
        Where-Object { $_.Program -and ([Environment]::ExpandEnvironmentVariables($_.Program) -ieq $program) } |
        Get-NetFirewallRule -ErrorAction SilentlyContinue |
        Where-Object { $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block' }
}

function Show-State {
    Write-Host "Executable: $Exe"
    foreach ($rule in $Rules) {
        $existing = Get-NetFirewallRule -Name $rule.Name -ErrorAction SilentlyContinue
        if ($existing) {
            $app = ($existing | Get-NetFirewallApplicationFilter).Program
            $state = if ($app -ieq $Exe) { 'present' } else { "present for another exe: $app" }
            Write-Host ("  {0,-48} {1} (enabled={2}, profile={3})" -f $rule.Display, $state, $existing.Enabled, $existing.Profile)
        } else {
            Write-Host ("  {0,-48} missing" -f $rule.Display)
        }
    }
    $blocks = @(Get-GameBlockRules $Exe)
    if ($blocks.Count) {
        Write-Warning "$($blocks.Count) inbound BLOCK rule(s) for this exe (a Cancel at the Windows prompt). Re-run with -RemoveBlockRules."
        $blocks | ForEach-Object { Write-Host "    block: $($_.DisplayName) [$($_.Profile)]" }
    }
    foreach ($net in Get-NetConnectionProfile -ErrorAction SilentlyContinue) {
        $note = if ($net.NetworkCategory -eq 'Public') {
            '  <- Public: these Private rules do not apply here. Settings > Network & internet > Wi-Fi > (your network) > Private network.'
        } else { '' }
        Write-Host ("  network '{0}' on {1}: {2}{3}" -f $net.Name, $net.InterfaceAlias, $net.NetworkCategory, $note)
    }
}

if ($Check) { Show-State; exit 0 }

$identity = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $identity.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'Firewall rules need administrator rights; Windows will ask for permission (UAC).'
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-Exe', "`"$Exe`"")
    if ($RemoveBlockRules) { $arguments += '-RemoveBlockRules' }
    if ($Remove) { $arguments += '-Remove' }
    $shell = (Get-Process -Id $PID).Path
    $child = Start-Process -FilePath $shell -ArgumentList $arguments -Verb RunAs -Wait -PassThru
    if ($child.ExitCode -ne 0) { Write-Warning "The elevated run exited with $($child.ExitCode)." }
    Show-State
    exit $child.ExitCode
}

foreach ($rule in $Rules) {
    Remove-NetFirewallRule -Name $rule.Name -ErrorAction SilentlyContinue
}
if ($Remove) {
    Write-Host 'Removed the two Panzerrechnersystem LAN rules.'
    Show-State
    Start-Sleep -Seconds 2
    exit 0
}
foreach ($rule in $Rules) {
    New-NetFirewallRule -Name $rule.Name -DisplayName $rule.Display -Group $Group `
        -Description 'Panzerrechnersystem LAN crew co-op (tools/setup_firewall.ps1). Private networks only.' `
        -Direction Inbound -Action Allow -Protocol UDP -LocalPort $rule.Port -Program $Exe `
        -Profile Private -Enabled True | Out-Null
    Write-Host "Added: $($rule.Display) for $Exe (Private networks)"
}
if ($RemoveBlockRules) {
    foreach ($block in @(Get-GameBlockRules $Exe)) {
        Remove-NetFirewallRule -Name $block.Name
        Write-Host "Removed block rule: $($block.DisplayName) [$($block.Profile)]"
    }
}
Show-State
Start-Sleep -Seconds 3
exit 0
