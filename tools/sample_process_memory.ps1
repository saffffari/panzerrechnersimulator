<#
.SYNOPSIS
  Sample a running Panzerrechnersystem process's memory every few seconds into a CSV (for
  tools/analyze_session_log.py --memory). Waits for the process to appear, stops when it exits.

.EXAMPLE
  powershell -File tools\sample_process_memory.ps1 -ExePath C:\shop\x\target\release\panzerrechnersystem.exe -Out run.memory.csv
#>
param(
    [Parameter(Mandatory)] [string]$ExePath,
    [Parameter(Mandatory)] [string]$Out,
    [int]$PeriodS = 5,
    [int]$WaitS = 120
)
$ErrorActionPreference = 'Stop'
$ExePath = [System.IO.Path]::GetFullPath($ExePath)
$deadline = (Get-Date).AddSeconds($WaitS)
$proc = $null
while (-not $proc -and (Get-Date) -lt $deadline) {
    $proc = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $ExePath } | Select-Object -First 1
    if (-not $proc) { Start-Sleep -Milliseconds 500 }
}
if (-not $proc) { throw "No process from $ExePath within $WaitS s" }
'timestamp,pid,elapsed_s,working_set_mb,private_mb,gpu_mem_used_mb,gpu_util_pct' | Set-Content $Out
$start = Get-Date
while (-not $proc.HasExited) {
    $proc.Refresh()
    $gpu = (& nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader,nounits 2>$null) -split ',\s*'
    $line = '{0},{1},{2:F1},{3:F1},{4:F1},{5},{6}' -f (Get-Date -Format o), $proc.Id, ((Get-Date) - $start).TotalSeconds,
        ($proc.WorkingSet64 / 1MB), ($proc.PrivateMemorySize64 / 1MB), $gpu[0], $gpu[1]
    Add-Content $Out $line
    Start-Sleep -Seconds $PeriodS
}
