<#
.SYNOPSIS
  Copy every screenshot and video from the main checkout and all its worktrees into two flat
  folders: <main checkout>\captures\screenshots and <main checkout>\captures\videos.

.DESCRIPTION
  The owner browses captures as one flat set sorted by date modified. Worktrees and older harnesses
  write into their own captures\ and artifacts\ trees (often in subfolders), so this tool gathers them:

  - Copies only; originals stay where tests, manifests and docs expect them.
  - Keeps each file's LastWriteTime, so "sort by date modified" shows true capture order.
  - Flattens names: <worktree>-<subfolders>-<file>. Files directly in the main checkout's
    captures\ keep their name. No subfolders are ever created in the two destinations.
  - Skips a file already copied with the same size and timestamp; a newer source replaces it.
  - Never collects assets\, dist\, target\, .git\ or gallery thumbnails.
  The destinations are inside the git-ignored captures\ folder: nothing here is ever committed.

.PARAMETER MainCheckout
  The main checkout (default: the git common directory's parent, i.e. C:\shop\panzerrechnersystem).

.PARAMETER Since
  Only gather files modified on or after this date (default: everything).

.PARAMETER WhatIf
  List what would be copied without copying.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\sync_captures.ps1
  powershell -ExecutionPolicy Bypass -File tools\sync_captures.ps1 -Since 2026-10-27
#>
param(
    [string]$MainCheckout,
    [datetime]$Since = [datetime]::MinValue,
    [switch]$WhatIf
)
$ErrorActionPreference = 'Stop'

if (-not $MainCheckout) {
    $common = & git -C $PSScriptRoot rev-parse --path-format=absolute --git-common-dir 2>$null
    $MainCheckout = if ($common) { Split-Path $common.Trim() } else { (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }
}
$MainCheckout = (Resolve-Path $MainCheckout).Path
$Shots  = Join-Path $MainCheckout 'captures\screenshots'
$Videos = Join-Path $MainCheckout 'captures\videos'
New-Item -ItemType Directory -Force $Shots, $Videos | Out-Null

$imageExt = @('.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp')
$videoExt = @('.mp4', '.webm', '.mov', '.mkv', '.avi')
$skipDirs = '\\(target|dist|assets|\.git|thumbs|node_modules)(\\|$)|\\captures\\(screenshots|videos|gallery_[^\\]*)(\\|$)'

# The main checkout plus every worktree git knows about.
$trees = @($MainCheckout)
$porcelain = & git -C $MainCheckout worktree list --porcelain 2>$null
foreach ($line in $porcelain) {
    if ($line -like 'worktree *') {
        $p = ($line.Substring(9)).Replace('/', '\')
        if ((Test-Path $p) -and ($trees -notcontains $p)) { $trees += $p }
    }
}

function Get-Prefix([string]$tree) {
    if ($tree -eq $MainCheckout) { return '' }
    $leaf = Split-Path $tree -Leaf
    $mainLeaf = Split-Path $MainCheckout -Leaf
    if ($leaf.StartsWith("$mainLeaf-")) { $leaf = $leaf.Substring($mainLeaf.Length + 1) }
    return $leaf
}

$copied = 0; $skipped = 0; $seen = 0
foreach ($tree in $trees) {
    $prefix = Get-Prefix $tree
    foreach ($sub in 'captures', 'artifacts') {
        $base = Join-Path $tree $sub
        if (-not (Test-Path $base)) { continue }
        Get-ChildItem -LiteralPath $base -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
            $ext = $_.Extension.ToLowerInvariant()
            ($imageExt -contains $ext -or $videoExt -contains $ext) -and
            $_.LastWriteTime -ge $Since -and
            $_.FullName.Substring($tree.Length) -notmatch $skipDirs
        } | ForEach-Object {
            $seen++
            $ext = $_.Extension.ToLowerInvariant()
            $dest = if ($videoExt -contains $ext) { $Videos } else { $Shots }
            # Relative folders below the scanned root become part of the name; the main checkout's
            # own captures\ root files keep their names exactly.
            $relDir = $_.DirectoryName.Substring($base.Length).Trim('\')
            $parts = @()
            if ($prefix) { $parts += $prefix }
            if ($sub -eq 'artifacts') { $parts += 'artifacts' }
            if ($relDir) { $parts += ($relDir -split '\\') }
            $parts += $_.Name
            $name = ($parts -join '-') -replace '[<>:"/|?*]', '_'
            $target = Join-Path $dest $name
            $existing = Get-Item -LiteralPath $target -ErrorAction SilentlyContinue
            if ($existing -and $existing.Length -eq $_.Length -and $existing.LastWriteTime -eq $_.LastWriteTime) { $skipped++; return }
            if ($WhatIf) { Write-Host "would copy $($_.FullName) -> $target"; $copied++; return }
            Copy-Item -LiteralPath $_.FullName -Destination $target -Force
            (Get-Item -LiteralPath $target).LastWriteTime = $_.LastWriteTime
            $copied++
        }
    }
}
Write-Host ("sync_captures: {0} media files seen in {1} checkouts; {2} copied, {3} already current." -f $seen, $trees.Count, $copied, $skipped)
Write-Host "screenshots: $Shots ($((Get-ChildItem $Shots -File).Count) files)"
Write-Host "videos:      $Videos ($((Get-ChildItem $Videos -File).Count) files)"
