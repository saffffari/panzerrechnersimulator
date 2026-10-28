<#
.SYNOPSIS
  Build the release exe and assemble the portable folder dist\Panzerrechnersystem-win64\.

.DESCRIPTION
  For copying the game to the owner's own Windows laptops by USB stick or a network share, for LAN
  crew co-op on the same Wi-Fi (docs\LAN_WINDOWS.md). The folder holds:

    panzerrechnersystem.exe              the release build of this checkout
    tools\launch_windows.ps1              launcher (-Host / -Join / -Laptop tiers)
    tools\setup_firewall.ps1              the two inbound UDP rules, Private networks only
    assets\ catalog\                      exactly the runtime assets the game loads, at their
                                          logical paths (tools\windows_payload.py)
    payload.windows.v1.json               the copied files with sizes and SHA-256
    *.cmd                                 double-click shortcuts (host, join, firewall, menu)
    README.txt, LAN_WINDOWS.md            setup and troubleshooting

  OWNER-PRIVATE: the asset library is purchased kits (Pizza Doggy and others). The folder is for
  the owner's own machines. It is git-ignored (/dist/); never upload it, attach it, or put it on
  a public share. This script uploads nothing.

.PARAMETER AssetRoot
  Checkout that holds the git-ignored assets (default: this checkout, or the main checkout when
  this is a git worktree without its own assets).

.PARAMETER Out
  Output folder (default: dist\Panzerrechnersystem-win64). Refuses to write into a non-empty one.

.PARAMETER Trace
  KESSEL_ASSET_TRACE file(s) from real play sessions: every present path they list joins the copy.

.PARAMETER SkipBuild
  Package the existing target\release\panzerrechnersystem.exe.

.PARAMETER DryRun
  Report the asset closure and its size; build and copy nothing.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\package_windows.ps1
  powershell -ExecutionPolicy Bypass -File tools\package_windows.ps1 -DryRun
  powershell -ExecutionPolicy Bypass -File tools\package_windows.ps1 -Trace artifacts\runtime\trace-canyon.txt
#>
[CmdletBinding()]
param(
    [string]$AssetRoot,
    [string]$Out,
    [string[]]$Trace = @(),
    [switch]$SkipBuild,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Binary = Join-Path $Target 'release\panzerrechnersystem.exe'
if (-not $Out) { $Out = Join-Path $Root 'dist\Panzerrechnersystem-win64' }
if (-not $AssetRoot) {
    $AssetRoot = $Root
    if (-not (Test-Path (Join-Path $Root 'assets\tanks\tiger\baked\tiger_hull.glb'))) {
        $common = & git -C $Root rev-parse --path-format=absolute --git-common-dir 2>$null
        if ($common) { $AssetRoot = Split-Path $common.Trim() }
    }
}
$AssetRoot = (Resolve-Path $AssetRoot).Path
$python = @('python', 'python3', 'py') | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } | Select-Object -First 1
if (-not $python) { throw 'Python 3 is required (tools\windows_payload.py).' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
}

$payloadArgs = @((Join-Path $Root 'tools\windows_payload.py'), '--asset-root', $AssetRoot, '--repo-root', $Root)
foreach ($t in $Trace) { $payloadArgs += '--trace', (Resolve-Path $t).Path }
Write-Host "Asset root: $AssetRoot"
if ($DryRun) {
    & $python @payloadArgs --dry-run
    exit $LASTEXITCODE
}

if ((Test-Path $Out) -and (Get-ChildItem $Out -Force | Select-Object -First 1)) {
    throw "Refusing to write into non-empty $Out (delete it or pass -Out <new folder>)."
}
# Keep the copy inside the git-ignored dist\ unless the owner names another folder on purpose.
$distRoot = Join-Path $Root 'dist'
if ($Out.StartsWith($Root, [StringComparison]::OrdinalIgnoreCase) -and -not $Out.StartsWith($distRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Inside the checkout the package must go under dist\ (git-ignored): $Out"
}

if (-not $SkipBuild) {
    Write-Host 'Building release binary (cargo build --release --locked) ...'
    Push-Location $Root
    try { & cargo build --release --locked --bin panzerrechnersystem } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path $Binary)) { throw "Release binary missing: $Binary" }

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
Write-Host "Copying the runtime asset closure into $Out ..."
& $python @payloadArgs --output $Out
if ($LASTEXITCODE -ne 0) { throw "windows_payload.py failed with exit code $LASTEXITCODE" }

Copy-Item $Binary (Join-Path $Out 'panzerrechnersystem.exe')
New-Item -ItemType Directory -Force (Join-Path $Out 'tools') | Out-Null
foreach ($script in 'launch_windows.ps1', 'setup_firewall.ps1') {
    Copy-Item (Join-Path $Root "tools\$script") (Join-Path $Out "tools\$script")
}
Copy-Item (Join-Path $Root 'docs\LAN_WINDOWS.md') (Join-Path $Out 'LAN_WINDOWS.md')

$launch = '@powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launch_windows.ps1"'
$shortcuts = [ordered]@{
    'Host Canyon.cmd'          = "$launch -Host Canyon %*"
    'Host Campaign.cmd'        = "$launch -Host Campaign %*"
    'Host Arena.cmd'           = "$launch -Host Arena %*"
    'Join.cmd'                 = "@set /p ADDR=Host address shown on the host's screen (or auto): `r`n$launch -Join %ADDR% %*"
    'Menu.cmd'                 = "$launch -Mode menu %*"
    'Allow LAN in firewall.cmd' = '@powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\setup_firewall.ps1" %*'
}
foreach ($name in $shortcuts.Keys) {
    [System.IO.File]::WriteAllText((Join-Path $Out $name), $shortcuts[$name] + "`r`n")
}

$stream = [System.IO.File]::OpenRead((Join-Path $Out 'panzerrechnersystem.exe'))
try { $sha = ([System.BitConverter]::ToString([System.Security.Cryptography.SHA256]::Create().ComputeHash($stream)) -replace '-', '').ToLower() }
finally { $stream.Dispose() }
$commit = (& git -C $Root log -1 --format='%h %s' 2>$null)
$payload = Get-Content (Join-Path $Out 'payload.windows.v1.json') -Raw | ConvertFrom-Json
$readme = @"
Panzerrechnersystem - portable Windows folder
=============================================

Build: $commit
panzerrechnersystem.exe SHA-256: $sha
Assets: $($payload.file_count) files, $([math]::Round($payload.total_bytes / 1GB, 2)) GB

PRIVATE COPY. The models and sounds are purchased kits licensed to the owner. Copy this folder only
to your own laptops (USB stick, external drive, or a share on your own home network). Do not
upload it, attach it, or share it publicly.

Copy the whole folder (about $([math]::Round($payload.total_bytes / 1GB, 1)) GB) to each laptop, e.g. to
C:\Games\Panzerrechnersystem-win64. Saves and settings go to %LOCALAPPDATA%\Panzerrechnersystem.

Crew co-op on one Wi-Fi (every laptop on the same network, marked Private):
  1. On the HOST laptop, once: double-click "Allow LAN in firewall.cmd" and accept the UAC prompt.
  2. HOST: double-click "Host Canyon.cmd" (or Host Campaign / Host Arena). The top-left of the
     screen shows FRIENDS JOIN <address> until somebody joins.
  3. Each FRIEND: double-click "Join.cmd" and type that address (or "auto" to find it).
     If Windows asks whether to allow the game on networks, choose Private networks and Allow.
  The host's seat is Commander; friends get the next free seat (F6 asks for another). The AI crew
  mans every seat nobody holds and steps aside when a friend takes it.

Laptops: the launcher picks the laptop profile (480x272, laptop preset) unless an RTX 4080/4090/
5080/5090 desktop card is present. Integrated graphics (Intel UHD/Iris Xe, Radeon iGPU) are far too
slow for this game; a laptop needs a discrete GPU. See LAN_WINDOWS.md, "Laptop GPUs".

Troubleshooting (LAN_WINDOWS.md has the full list):
  "NO ANSWER FROM ..."      same Wi-Fi? host firewall step 1? guest network / client isolation?
  "... IS UP BUT NOT HOSTING"  the host has not started hosting yet.
  Diagnostics without the game window:
     panzerrechnersystem.exe lan-ifaces              this laptop's addresses and broadcasts
     panzerrechnersystem.exe lan-probe join <addr>   test the connection to a host
     tools\setup_firewall.ps1 -Check                 the firewall rules and network category
"@
[System.IO.File]::WriteAllText((Join-Path $Out 'README.txt'), $readme.Replace("`n", "`r`n"))
Write-Host ""
Write-Host "Package: $Out"
Write-Host "  exe SHA-256 $sha"
Write-Host "  $($payload.file_count) asset files, $([math]::Round($payload.total_bytes / 1MB)) MB"
Write-Host "Owner-private: copy to your own laptops only; nothing was uploaded."
