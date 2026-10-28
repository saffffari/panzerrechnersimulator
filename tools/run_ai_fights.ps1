# Headless AI-vs-AI combat captures (video via ffmpeg) from this checkout's release binary: a varied set of
# showreel, crew-battle, blizzard, duel, brawl and canyon autopilot runs. Each run gets its own log; a run that
# fails on the raymarch backend is retried once on the shader backend. Needs ffmpeg on PATH and a GPU that
# nothing else is using. Usage: powershell -File tools\run_ai_fights.ps1 [-AssetRoot <checkout with assets>]
param([string]$AssetRoot)
$ErrorActionPreference = 'Continue'
$Root   = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Assets = if ($AssetRoot) { $AssetRoot } elseif ($env:KESSEL_ASSET_ROOT) { $env:KESSEL_ASSET_ROOT } else { $Root }
$Exe    = "$Root\target\release\panzerrechnersystem.exe"
$Out    = "$Root\captures\aivai"
$Logs   = "$Root\artifacts\runtime\aivai"
New-Item -ItemType Directory -Force $Out, $Logs | Out-Null
$env:Path = "$env:LOCALAPPDATA\Microsoft\WinGet\Links;$env:Path"
Get-ChildItem Env: | Where-Object { $_.Name -like 'KESSEL_*' } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
$env:HOME = $env:USERPROFILE
$env:XDG_STATE_HOME = "$Root\artifacts\runtime\state"; $env:XDG_CACHE_HOME = "$Root\artifacts\runtime\cache"; $env:XDG_CONFIG_HOME = "$Root\artifacts\runtime\config"
$base = @{
  KESSEL_ASSET_ROOT = $Assets; KESSEL_GRAPHICS_PRESET = 'high'; KESSEL_INTERNAL_RES = '800x450'
  KESSEL_DITHER_PATTERN = 'screen_bayer8'; KESSEL_GEOMETRY_AA = 'msaa4'; KESSEL_SOURCE_VISIBILITY = 'maps'
  KESSEL_ENABLE_PIZZA_AUDIO = '1'; KESSEL_TRANSPORT_QUALITY = 'full'; KESSEL_AUDIO_CAPTURE = '1'
}
$runs = @(
  @{ name = 'showreel_3t34_arena_overcast';  args = @('ai-showreel', "$Out\showreel_3t34_arena", 'assets/ai/weights_academy.bin', 'assets/ai/weights_tiger.bin', '3', '120', '0'); env = @{ KESSEL_SHOT_WEATHER = 'clouds' } },
  @{ name = 'showreel_6t34_arena_storm';     args = @('ai-showreel', "$Out\showreel_6t34_arena", 'assets/ai/weights_academy.bin', 'assets/ai/weights_tiger.bin', '6', '150', '0'); env = @{ KESSEL_SHOT_WEATHER = 'storm' } },
  @{ name = 'showreel_auto_act1_dusk';       args = @('ai-showreel', "$Out\showreel_auto_act1", 'assets/ai/weights_academy.bin', 'assets/ai/weights_tiger.bin', 'auto', '150', '1'); env = @{ KESSEL_SHOT_WEATHER = 'clear'; KESSEL_SKY_UTC = '1942-12-21T13:30:00Z' } },
  @{ name = 'crew_battle_platoon';           args = @('crew-battle', "$Out\crew_battle", 'platoon', '20261027', '40'); env = @{} },
  @{ name = 'blizzard_cupola_platoon';       args = @('blizzard-battle', "$Out\blizzard_cupola", '30', 'cupola', 'platoon'); env = @{} },
  @{ name = 'blizzard_inside';               args = @('blizzard-battle', "$Out\blizzard_inside", '30', 'inside'); env = @{} },
  @{ name = 'duel_arena_night';              args = @('duel', "$Out\duel_night", 'assets/ai/weights_tiger.bin', '0', '150'); env = @{ KESSEL_SHOT_WEATHER = 'clear'; KESSEL_SKY_UTC = '1942-12-21T20:00:00Z' } },
  @{ name = 'brawl';                         args = @('brawl', "$Out\brawl"); env = @{} },
  @{ name = 'canyon_ai_run_seed29';          args = @('canyon-ai-run', "$Out\canyon_ai_29", '29', '240'); env = @{ KESSEL_CANYON_V2 = '1' } }
)
$summary = @()
foreach ($r in $runs) {
  foreach ($backend in @('raymarch', 'shader')) {
    foreach ($k in $base.Keys) { Set-Item "Env:$k" $base[$k] }
    foreach ($k in $r.env.Keys) { Set-Item "Env:$k" $r.env[$k] }
    foreach ($k in @('KESSEL_SHOT_WEATHER','KESSEL_SKY_UTC','KESSEL_CANYON_V2')) { if (-not $r.env.ContainsKey($k)) { Remove-Item "Env:$k" -ErrorAction SilentlyContinue } }
    $env:KESSEL_RENDERER = $backend
    if ($backend -eq 'shader') { $env:KESSEL_NO_RT = '1' } else { Remove-Item Env:KESSEL_NO_RT -ErrorAction SilentlyContinue }
    $log = "$Logs\$($r.name)_$backend.log"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $Exe -ArgumentList $r.args -WorkingDirectory $Root -PassThru -NoNewWindow -RedirectStandardOutput "$log.out" -RedirectStandardError "$log.err"
    $null = $p.Handle
    $ok = $p.WaitForExit(1200000)
    if (-not $ok) { Stop-Process -Id $p.Id -Force; $code = 'TIMEOUT' } else { $code = $p.ExitCode }
    Get-Content "$log.out", "$log.err" -ErrorAction SilentlyContinue | Set-Content $log
    Remove-Item "$log.out", "$log.err" -ErrorAction SilentlyContinue
    $line = "{0,-32} {1,-8} exit={2,-7} {3,5}s" -f $r.name, $backend, $code, [int]$sw.Elapsed.TotalSeconds
    Write-Host $line; $summary += $line
    if ($code -eq 0) { break }
  }
}
"`n=== outputs"; Get-ChildItem $Out -Recurse -File | Where-Object { $_.Extension -in '.mp4', '.png' } | Sort-Object FullName | ForEach-Object { "{0,10} KB  {1}" -f [int]($_.Length / 1KB), $_.FullName.Replace($Out + '\', '') }
"`n=== summary"; $summary
