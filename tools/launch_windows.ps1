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

.PARAMETER Resolution
  Internal render grid: 800x450 (default), 640x360, 480x272 or 320x180. The window is unchanged;
  the frame is upscaled to it. Passing -Resolution also writes the grid into the saved graphics
  preferences (which otherwise win at startup); the graphics menu offers the same grids in play.
  Measured frame costs per grid are in docs/RAYMARCHING_ENGINE.md ("Internal resolution").
  -InternalRes is an alias (integration-1 telemetry runs).

.PARAMETER Logged
  Playtest telemetry (port of tools/launch_session_logged_playtest.sh): one JSON line per second
  (fps, frame-time mean/p50/p95/p99/max, CPU simulation and render-submission time, render work
  counts, gameplay snapshot) to artifacts\runtime\session-<stamp>.jsonl, with KESSEL_GPU_PROFILE=1
  so one fenced frame every KESSEL_SESSION_LOG_GPU_S seconds (default 2) records per-pass GPU time.
  The console goes to session-<stamp>.console.log beside it; session-latest.* copies follow.

.PARAMETER Autopilot
  The rule-based Canyon crew (src/canyon_autopilot.rs) plays the run instead of the keyboard;
  the process exits once the after-action card has shown (0 completed, 1 knocked out,
  3 stalled, 4 time limit). Meant for -Logged telemetry runs.

.PARAMETER Seed
  Pin the Canyon deployment seed (KESSEL_CANYON_SEED) for a reproducible run.

.PARAMETER AssetRoot
  Where the git-ignored assets live (default: this checkout, or the main checkout when this is a
  git worktree without its own assets). A Canyon v2 workshop inside this checkout still wins.

.PARAMETER Binary
  Release executable (default: <CARGO_TARGET_DIR or target>\release\panzerrechnersystem.exe).

.PARAMETER NoVsync
  Present uncapped (a vsync-off copy of the saved graphics preferences) so frame times show the
  real headroom rather than the display's refresh interval.

.PARAMETER Host
  LAN crew co-op (docs/LAN_WINDOWS.md): host a Tiger for friends on the same Wi-Fi. Campaign,
  Arena, Garage or Canyon (the Canyon v2 winter run). The host's screen shows the address friends
  type; run tools\setup_firewall.ps1 once on the host first.

.PARAMETER Join
  LAN crew co-op: join a host by its address (192.168.1.50 or 192.168.1.50:28460), or `auto` to
  join the first compatible Tiger heard on the discovery port.

.PARAMETER Role
  Seat to ask for in LAN play (commander, driver, gunner, loader, radio). The host arbitrates;
  F6 in play asks for the next free seat. Default: commander for the host, gunner for a client.

.PARAMETER Name
  Callsign shown to the crew (default: the Windows user name).

.PARAMETER Port
  LAN gameplay UDP port (default 28460). Discovery stays on 28461.

.PARAMETER Tier
  Graphics profile. auto (default) picks from the GPU: the 4090 profile on an RTX 4080/4090/5080/
  5090 class card, else the laptop profile. laptop: 480x272 internal grid, laptop preset, gameplay
  transport sampling, no MSAA, the shader backend unless a ray-tracing laptop GPU (RTX 30/40/50,
  Radeon RX 6000/7000, Arc A) is present, in which case raymarch. 4090: the profile above.
  A tier changes resolution and sampling only, never the scene or the simulation (CLAUDE.md).
  The laptop profile keeps its own saved preferences (graphics.laptop.v1.json).

.PARAMETER Laptop
  Shorthand for -Tier laptop.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Mode menu -Windowed
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Renderer raymarch
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Renderer raymarch -Logged -Windowed
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Renderer raymarch -Logged -Autopilot -Seed 29
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Renderer raymarch -Resolution 480x272
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Host Canyon
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Join 192.168.1.50 -Role gunner -Laptop
  powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Join auto -Windowed
#>
[CmdletBinding()]
param(
    [ValidateSet('play', 'menu', 'check')] [string]$Mode = 'play',
    [switch]$Windowed,
    [switch]$Build,
    [switch]$Platoon,
    [ValidateSet('shader', 'raymarch')] [string]$Renderer = 'shader',
    [switch]$Logged,
    [switch]$Autopilot,
    [string]$Seed,
    [Alias('InternalRes')] [ValidateSet('800x450', '640x360', '480x272', '320x180')] [string]$Resolution = '800x450',
    [string]$AssetRoot,
    [string]$Binary,
    [switch]$NoVsync,
    # `$Host` is PowerShell's own console object; the switch is -Host, the variable $HostMission.
    [Alias('Host')] [ValidateSet('Campaign', 'Arena', 'Garage', 'Canyon')] [string]$HostMission,
    [string]$Join,
    [ValidateSet('commander', 'driver', 'gunner', 'loader', 'radio')] [string]$Role,
    [string]$Name,
    [ValidateRange(1, 65535)] [int]$Port = 28460,
    [ValidateSet('auto', 'laptop', '4090')] [string]$Tier = 'auto',
    [switch]$Laptop
)
$ErrorActionPreference = 'Stop'
Import-Module Microsoft.PowerShell.Utility, Microsoft.PowerShell.Management -ErrorAction SilentlyContinue
$Root     = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Target   = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
# The portable package (tools/package_windows.ps1) carries the exe at its root and no Cargo.toml.
$Packaged = (Test-Path (Join-Path $Root 'panzerrechnersystem.exe')) -and -not (Test-Path (Join-Path $Root 'Cargo.toml'))
if (-not $Binary) {
    $Binary = if ($Packaged) { Join-Path $Root 'panzerrechnersystem.exe' } else { Join-Path $Target 'release\panzerrechnersystem.exe' }
}
if ($HostMission -and $Join) { throw 'Choose either -Host or -Join.' }
$Lan = [bool]($HostMission -or $Join)
if ($Laptop) { $Tier = 'laptop' }
$WorkshopRel = 'assets\worldgen\workshops\gaea-canyon-run-v2\manifest.v1.json'
if (-not $AssetRoot) {
    $AssetRoot = $Root
    # A git worktree has no git-ignored assets of its own: read them from the main checkout.
    if (-not (Test-Path (Join-Path $Root 'assets\tanks\tiger\baked\tiger_hull.glb'))) {
        $common = & git -C $Root rev-parse --path-format=absolute --git-common-dir 2>$null
        if ($common) { $AssetRoot = Split-Path $common.Trim() }
    }
}
$AssetRoot = (Resolve-Path $AssetRoot).Path
# A Canyon v2 workshop snapshot inside this checkout (frozen for a run) wins over the asset root's.
$Manifest = Join-Path $Root $WorkshopRel
if (-not (Test-Path $Manifest)) { $Manifest = Join-Path $AssetRoot $WorkshopRel }
$Runtime  = Join-Path $Root 'artifacts\runtime'
# A package copied to a USB stick or a share may be read-only: its saves and logs live per user.
if ($Packaged) { $Runtime = Join-Path $env:LOCALAPPDATA 'Panzerrechnersystem' }
$Config   = Join-Path $Runtime 'winter-playtest-state\graphics.v1.json'

# --- graphics tier --------------------------------------------------------------------------
$gpus = @(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
if ($Tier -eq 'auto') {
    $Tier = if ($gpus | Where-Object { $_ -match 'RTX\s*(4080|4090|5080|5090)' -and $_ -notmatch 'Laptop' }) { '4090' } else { 'laptop' }
}
$GraphicsPreset = 'high'; $GeometryAA = 'msaa4'; $Transport = 'full'
if ($Tier -eq 'laptop') {
    $GraphicsPreset = 'laptop'; $GeometryAA = 'off'; $Transport = 'gameplay'
    if (-not $PSBoundParameters.ContainsKey('Resolution')) { $Resolution = '480x272' }
    if (-not $PSBoundParameters.ContainsKey('Renderer')) {
        $rtCapable = $gpus | Where-Object { $_ -match 'RTX\s*[2-5]0\d0|Radeon.*RX\s*[67]\d{3}|Arc.*A\d{3}' }
        $Renderer = if ($rtCapable) { 'raymarch' } else { 'shader' }
    }
    $Config = Join-Path $Runtime 'winter-playtest-state\graphics.laptop.v1.json'
}
Write-Host "GPU: $($gpus -join '; ')  ->  tier $Tier, renderer $Renderer, internal $Resolution"

Set-Location $Root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
}

if ($Mode -ne 'check' -and -not $Packaged -and ($Build -or -not (Test-Path $Binary))) {
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
$required += $doc.assets | ForEach-Object { Join-Path $AssetRoot $_ }
foreach ($k in 'texture', 'rock_map', 'snow_map', 'ground_class') { $required += Join-Path $AssetRoot $doc.terrain.$k }
foreach ($p in 'hull', 'turret', 'gun') { $required += Join-Path $AssetRoot "assets\tanks\tiger\baked\tiger_$p.glb" }
foreach ($s in 'json', 'png') { $required += Join-Path $AssetRoot "catalog\maps\canyon_v2_survey_20260930.$s" }
$missing = $required | Where-Object { -not (Test-Path $_ -PathType Leaf) -or (Get-Item $_).Length -eq 0 }
if ($missing) { throw ("Missing required assets:`n" + ($missing -join "`n")) }
Write-Host "Validated $($required.Count) required asset paths."
$stream = [System.IO.File]::OpenRead($Binary)
try { $sha = ([System.BitConverter]::ToString([System.Security.Cryptography.SHA256]::Create().ComputeHash($stream)) -replace '-', '').ToLower() }
finally { $stream.Dispose() }
Write-Host "Release: $Binary`nSHA256:  $sha"
if (Test-Path $Config) { Write-Host "Saved graphics preferences: $Config" }
else { Write-Host "First launch: $Resolution, screen_bayer8, fullscreen=$(-not $Windowed)." }
if ($Mode -eq 'check') { exit 0 }

# --- first-launch graphics preferences (afterwards only an explicit -Resolution changes them) ---
$grid = $Resolution.Split('x') | ForEach-Object { [int]$_ }
New-Item -ItemType Directory -Force (Split-Path $Config) | Out-Null
if (-not (Test-Path $Config)) {
    $initial = [ordered]@{
        version = 1; resolution = @($grid[0], $grid[1]); window = @(1280, 720)
        fullscreen = (-not $Windowed); vsync = $true; shadows = 2048
        full_transport = ($Transport -eq 'full'); optics = 1; fps = $false
        exposure_ev = 0.0; saturation = 1.0; warmth = 0.0; gamma = 1.0
        storm = $null; sun_phase = 0; sun_speed = 1.0; dither_pattern = 3
        display = [ordered]@{
            width = $grid[0]; height = $grid[1]; palette_levels = 31.0
            ordered_dither = 1.0; retro_strength = 1.0; ldr_dither = 1.0
            ambient_white_balance = $false; ambient_white_balance_strength = 1.0
        }
    }
    [System.IO.File]::WriteAllText($Config, (($initial | ConvertTo-Json -Depth 5) + "`n"))
}
elseif ($PSBoundParameters.ContainsKey('Resolution')) {
    # The saved preferences are applied at startup and would replace KESSEL_INTERNAL_RES.
    $saved = Get-Content $Config -Raw | ConvertFrom-Json
    $saved.resolution = @($grid[0], $grid[1])
    $saved.display.width = $grid[0]; $saved.display.height = $grid[1]
    [System.IO.File]::WriteAllText($Config, (($saved | ConvertTo-Json -Depth 5) + "`n"))
    Write-Host "Saved graphics preferences: render grid set to $Resolution."
}
if ($NoVsync) {
    # Uncapped presentation for headroom measurements: a copy of the saved preferences with
    # vsync off, rewritten each launch so the player's own file keeps its setting.
    $prefs = Get-Content $Config -Raw | ConvertFrom-Json
    $prefs.vsync = $false
    $Config = Join-Path (Split-Path $Config) 'graphics.novsync.v1.json'
    [System.IO.File]::WriteAllText($Config, (($prefs | ConvertTo-Json -Depth 5) + "`n"))
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

$env:KESSEL_ASSET_ROOT        = $AssetRoot
$env:KESSEL_CANYON_MANIFEST   = $Manifest
$env:KESSEL_RENDERER          = $Renderer
$env:KESSEL_GRAPHICS_PRESET   = $GraphicsPreset
$env:KESSEL_INTERNAL_RES      = $Resolution
$env:KESSEL_DITHER_PATTERN    = 'screen_bayer8'
$env:KESSEL_GEOMETRY_AA       = $GeometryAA
if (-not $Windowed) { $env:KESSEL_FULLSCREEN = '1' }
$env:KESSEL_SOURCE_VISIBILITY = 'maps'
$env:KESSEL_GRAPHICS_CONFIG   = $Config
$env:KESSEL_ENABLE_PIZZA_AUDIO = '1'
$env:KESSEL_TRANSPORT_QUALITY = $Transport
$env:KESSEL_CANYON_V2         = '1'
if ($Mode -eq 'play' -and -not $Lan) { $env:KESSEL_CANYON = '1' }
if ($Platoon) {
    $env:KESSEL_PLATOON = '2'
    if ($env:ANTHROPIC_API_KEY) { Write-Host 'Platoon radio: Claude model (ANTHROPIC_API_KEY set).' }
    else { Write-Host 'Platoon radio: offline keyword parser (no ANTHROPIC_API_KEY).' }
}
if ($Seed) { $env:KESSEL_CANYON_SEED = $Seed }
if ($Autopilot) { $env:KESSEL_CANYON_AUTOPILOT = '1' }

# --- LAN crew co-op arguments --------------------------------------------------------------------
$GameArgs = @()
if ($Lan) {
    if ($Join -and $Join -ieq 'auto') {
        Write-Host 'Listening 4 s for a Tiger on UDP 28461 ...'
        $heard = & $Binary lan-probe discover 4 2>&1 | ForEach-Object { "$_" }
        $line = $heard | Where-Object { $_ -match '^JOIN ' -and $_ -notmatch '\(protocol v' } | Select-Object -First 1
        if (-not $line) {
            $heard | Where-Object { $_ -match '^JOIN ' } | ForEach-Object { Write-Warning "Heard an incompatible build: $_" }
            throw ("No compatible Tiger answered on the discovery port. Ask the host for the address on " +
                "their screen and use -Join <address>; see docs\LAN_WINDOWS.md (firewall, client isolation).")
        }
        $Join = ($line -split ' ')[1]
        Write-Host "Found: $line"
    }
    if ($HostMission) {
        $GameArgs += '--host', '--mission', $HostMission.ToLower(), '--bind', "0.0.0.0:$Port"
        # A fresh deployment each hosting unless -Seed pins one; clients take the host's.
        $GameArgs += '--seed', $(if ($Seed) { $Seed } else { [string](Get-Random -Minimum 1 -Maximum 2147483647) })
        if (-not $Role) { $Role = 'commander' }
    } else {
        $address = if ($Join -match ':\d+$' -or $Port -eq 28460) { $Join } else { "${Join}:$Port" }
        $GameArgs += '--join', $address
        if (-not $Role) { $Role = 'gunner' }
    }
    $GameArgs += '--role', $Role
    if ($Name) { $GameArgs += '--name', $Name }
    if ($HostMission -eq 'Canyon' -and $Platoon) { Write-Host 'Platoon wingmen are host-only in LAN play (not replicated yet).' }
}

# --- launch -----------------------------------------------------------------------------------
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$log = Join-Path $Runtime "winter-playtest-$stamp-$PID.log"
if ($Logged) {
    # Telemetry: one JSON line per second; one fenced GPU pass-timing frame every two seconds.
    $env:KESSEL_SESSION_LOG = Join-Path $Runtime "session-$stamp.jsonl"
    $env:KESSEL_GPU_PROFILE = '1'
    $log = Join-Path $Runtime "session-$stamp.console.log"
    Write-Host "Session log:  $env:KESSEL_SESSION_LOG`nConsole log:  $log"
}
@(
    "[launcher] timestamp=$(Get-Date -Format o) mode=$Mode launcher_pid=$PID windowed=$Windowed"
    "[launcher] binary=$Binary sha256=$sha"
    "[launcher] checkout=$Root asset_root=$AssetRoot manifest=$Manifest"
    "[launcher] commit $(& git -C $Root log -1 --format='%h %s' 2>$null)"
    "[launcher] renderer=$Renderer tier=$Tier preset=$GraphicsPreset internal=$Resolution geometry_aa=$GeometryAA transport=$Transport visibility=maps config=$Config platoon=$Platoon"
    "[launcher] lan=$Lan args=$($GameArgs -join ' ')"
    "[launcher] logged=$Logged autopilot=$Autopilot seed=$Seed session_log=$env:KESSEL_SESSION_LOG"
) | Set-Content $log
$errLog = "$log.stderr"
$startArgs = @{ FilePath = $Binary; WorkingDirectory = $Root; PassThru = $true; NoNewWindow = $true
    RedirectStandardOutput = "$log.stdout"; RedirectStandardError = $errLog }
if ($GameArgs.Count) {
    # Windows PowerShell joins ArgumentList with spaces and no quoting: quote a callsign with spaces.
    $startArgs.ArgumentList = $GameArgs | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
}
$proc = Start-Process @startArgs
$null = $proc.Handle  # cache the handle so ExitCode is readable after exit
Write-Host "Panzerrechnersystem PID $($proc.Id); log: $log"
$proc.WaitForExit()
Get-Content "$log.stdout", $errLog -ErrorAction SilentlyContinue | Add-Content $log
Remove-Item "$log.stdout", $errLog -ErrorAction SilentlyContinue
Add-Content $log "[launcher] exit_status=$($proc.ExitCode) timestamp=$(Get-Date -Format o)"
if ($Logged) {
    # Windows symlinks need privileges; the latest pair are plain copies.
    Copy-Item $env:KESSEL_SESSION_LOG (Join-Path $Runtime 'session-latest.jsonl') -ErrorAction SilentlyContinue
    Copy-Item $log (Join-Path $Runtime 'session-latest.console.log') -ErrorAction SilentlyContinue
}
if ($proc.ExitCode -ne 0) { Write-Warning "Panzerrechnersystem exited with status $($proc.ExitCode). Log: $log" }
exit $proc.ExitCode
