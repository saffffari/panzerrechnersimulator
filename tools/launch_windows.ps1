<#
.SYNOPSIS
  Windows launcher for Panzerrechnersystem (port of tools/launch_winter_playtest.sh).

.DESCRIPTION
  Starts the MAIN release build from this checkout with the fixed 12900K / RTX 4090 profile:
  shader renderer, high preset, 800x450 internal grid, screen Bayer-8 dither, MSAA 4, fullscreen.
  Saves, caches and graphics preferences stay inside artifacts/runtime so nothing touches
  the Linux-style ~/.local, ~/.cache or ~/.config layout the engine expects on Linux.

.PARAMETER Mode
  play   Start a fresh Canyon v2 winter run (default).
  menu   Open the title menu (graphics settings, Canyon v2, singleplayer, co-op).
  check  Read-only binary/asset checks; never builds or starts the game.

.PARAMETER Windowed
  Start in a maximized 1280x720 window instead of borderless fullscreen.

.PARAMETER Build
  Run `cargo build --release --locked --bin panzerrechnersystem` first (also done
  automatically when the binary is missing).

.PARAMETER Platoon
  Platoon mode (docs/PLATOON_MODE.md): two AI Tigers under your command on the Canyon v2 run,
  ordered over the radio (T opens the typed line). Sets KESSEL_PLATOON=2. With ANTHROPIC_API_KEY
  in the environment the radio goes through the Claude model; without it an offline parser
  answers.

.PARAMETER Renderer
  shader   The raster/hybrid shader backend (default; the Linux playtest profile).
  raymarch The froxel ray-marching backend (docs/RAYMARCHING_ENGINE.md): clustered
           lights, marched media, fireballs, embers, reflections, eyepiece glass.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Mode menu -Windowed
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Renderer raymarch
#>
[CmdletBinding()]
param(
    [ValidateSet('play', 'menu', 'check')] [string]$Mode = 'play',
    [switch]$Windowed,
    [switch]$Build,
    [switch]$Platoon,
    [ValidateSet('shader', 'raymarch')] [string]$Renderer = 'shader'
)
$ErrorActionPreference = 'Stop'
Import-Module Microsoft.PowerShell.Utility, Microsoft.PowerShell.Management -ErrorAction SilentlyContinue
$Root     = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Binary   = Join-Path $Root 'target\release\panzerrechnersystem.exe'
$Manifest = Join-Path $Root 'assets\worldgen\workshops\gaea-canyon-run-v2\manifest.v1.json'
$Runtime  = Join-Path $Root 'artifacts\runtime'
$Config   = Join-Path $Runtime 'winter-playtest-state\graphics.v1.json'

Set-Location $Root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
}

if ($Mode -ne 'check' -and ($Build -or -not (Test-Path $Binary))) {
    Write-Host "Building release binary (cargo build --release --locked) ..."
    & cargo build --release --locked --bin panzerrechnersystem
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path $Binary)) { throw "Release binary missing: $Binary (run with -Build)" }

# --- read-only validation, same closure as the Linux launcher's python block -------------------
$doc = Get-Content $Manifest -Raw | ConvertFrom-Json
if ($doc.act_id -ne 1 -or $doc.terrain.grass_season -ne 'winter') {
    throw "Expected the authored winter Act-I Canyon v2 manifest: $Manifest"
}
$required = @()
$required += Join-Path (Split-Path $Manifest) $doc.terrain.heights
$required += $doc.assets | ForEach-Object { Join-Path $Root $_ }
foreach ($k in 'texture', 'rock_map', 'snow_map', 'ground_class') { $required += Join-Path $Root $doc.terrain.$k }
foreach ($p in 'hull', 'turret', 'gun') { $required += Join-Path $Root "assets\tanks\tiger\baked\tiger_$p.glb" }
foreach ($s in 'json', 'png') { $required += Join-Path $Root "catalog\maps\canyon_v2_survey_20260930.$s" }
$missing = $required | Where-Object { -not (Test-Path $_ -PathType Leaf) -or (Get-Item $_).Length -eq 0 }
if ($missing) { throw ("Missing required assets:`n" + ($missing -join "`n")) }
Write-Host "Validated $($required.Count) required asset paths."
$stream = [System.IO.File]::OpenRead($Binary)
try { $sha = ([System.BitConverter]::ToString([System.Security.Cryptography.SHA256]::Create().ComputeHash($stream)) -replace '-', '').ToLower() }
finally { $stream.Dispose() }
Write-Host "Release: $Binary`nSHA256:  $sha"
if (Test-Path $Config) { Write-Host "Saved graphics preferences: $Config" }
else { Write-Host "First launch: 800x450, screen_bayer8, fullscreen=$(-not $Windowed)." }
if ($Mode -eq 'check') { exit 0 }

# --- first-launch graphics preferences (never overwritten afterwards) --------------------------
New-Item -ItemType Directory -Force (Split-Path $Config) | Out-Null
if (-not (Test-Path $Config)) {
    $initial = [ordered]@{
        version = 1; resolution = @(800, 450); window = @(1280, 720)
        fullscreen = (-not $Windowed); vsync = $true; shadows = 2048
        full_transport = $true; optics = 1; fps = $false
        exposure_ev = 0.0; saturation = 1.0; warmth = 0.0; gamma = 1.0
        storm = $null; sun_phase = 0; sun_speed = 1.0; dither_pattern = 3
        display = [ordered]@{
            width = 800; height = 450; palette_levels = 31.0
            ordered_dither = 1.0; retro_strength = 1.0; ldr_dither = 1.0
            ambient_white_balance = $false; ambient_white_balance_strength = 1.0
        }
    }
    [System.IO.File]::WriteAllText($Config, (($initial | ConvertTo-Json -Depth 5) + "`n"))
}

# --- environment ------------------------------------------------------------------------------
# Drop inherited experiment flags so a stale shell cannot change the profile.
Get-ChildItem Env: | Where-Object { $_.Name -like 'KESSEL_*' } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
# The engine resolves saves/caches/config through HOME/XDG_*; keep them inside the checkout.
if (-not $env:HOME) { $env:HOME = $env:USERPROFILE }
$env:XDG_STATE_HOME  = Join-Path $Runtime 'state'
$env:XDG_CACHE_HOME  = Join-Path $Runtime 'cache'
$env:XDG_CONFIG_HOME = Join-Path $Runtime 'config'
foreach ($d in $env:XDG_STATE_HOME, $env:XDG_CACHE_HOME, $env:XDG_CONFIG_HOME) { New-Item -ItemType Directory -Force $d | Out-Null }

$env:KESSEL_ASSET_ROOT        = $Root
$env:KESSEL_RENDERER          = $Renderer
$env:KESSEL_GRAPHICS_PRESET   = 'high'
$env:KESSEL_INTERNAL_RES      = '800x450'
$env:KESSEL_DITHER_PATTERN    = 'screen_bayer8'
$env:KESSEL_GEOMETRY_AA       = 'msaa4'
if (-not $Windowed) { $env:KESSEL_FULLSCREEN = '1' }
$env:KESSEL_SOURCE_VISIBILITY = 'maps'
$env:KESSEL_GRAPHICS_CONFIG   = $Config
$env:KESSEL_ENABLE_PIZZA_AUDIO = '1'
$env:KESSEL_TRANSPORT_QUALITY = 'full'
$env:KESSEL_CANYON_V2         = '1'
if ($Mode -eq 'play') { $env:KESSEL_CANYON = '1' }
if ($Platoon) {
    $env:KESSEL_PLATOON = '2'
    if ($env:ANTHROPIC_API_KEY) { Write-Host 'Platoon radio: Claude model (ANTHROPIC_API_KEY set).' }
    else { Write-Host 'Platoon radio: offline keyword parser (no ANTHROPIC_API_KEY).' }
}

# --- launch -----------------------------------------------------------------------------------
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$log = Join-Path $Runtime "winter-playtest-$stamp-$PID.log"
@(
    "[launcher] timestamp=$(Get-Date -Format o) mode=$Mode launcher_pid=$PID windowed=$Windowed"
    "[launcher] binary=$Binary sha256=$sha"
    "[launcher] asset_root=$Root manifest=$Manifest"
    "[launcher] renderer=$Renderer preset=high geometry_aa=msaa4 visibility=maps config=$Config platoon=$Platoon"
) | Set-Content $log
$errLog = "$log.stderr"
$proc = Start-Process -FilePath $Binary -WorkingDirectory $Root -PassThru -NoNewWindow `
    -RedirectStandardOutput "$log.stdout" -RedirectStandardError $errLog
$null = $proc.Handle  # cache the handle so ExitCode is readable after exit
Write-Host "Panzerrechnersystem PID $($proc.Id); log: $log"
$proc.WaitForExit()
Get-Content "$log.stdout", $errLog -ErrorAction SilentlyContinue | Add-Content $log
Remove-Item "$log.stdout", $errLog -ErrorAction SilentlyContinue
Add-Content $log "[launcher] exit_status=$($proc.ExitCode) timestamp=$(Get-Date -Format o)"
if ($proc.ExitCode -ne 0) { Write-Warning "Panzerrechnersystem exited with status $($proc.ExitCode). Log: $log" }
exit $proc.ExitCode
