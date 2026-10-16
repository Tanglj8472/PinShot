# mem-probe.ps1 - PinShot runtime memory sampler (zero-intrusion, no code changes)
#
# NOTE: keep this file pure ASCII. Windows PowerShell 5.1 reads scripts as
# ANSI/GBK unless a BOM is present, and CJK bytes inside string literals
# corrupt the parser.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File tools\mem-probe.ps1 -Seconds 60 -Out mem.csv
#   powershell -ExecutionPolicy Bypass -File tools\mem-probe.ps1 -Seconds 30 -Quiet
param(
  [int]$Seconds = 60,
  [int]$IntervalMs = 200,
  [string]$ProcName = "pinshot",
  [string]$Out = "",
  [switch]$Quiet
)

$ErrorActionPreference = "Stop"

$fields = @(
  "WorkingSet64", "PrivateMemorySize64", "VirtualMemorySize64",
  "PagedMemorySize64", "PeakWorkingSet64", "PeakPagedMemorySize64",
  "HandleCount", "ReadOperationCount", "WriteOperationCount"
)

function Get-Snap($p) {
  $o = [ordered]@{
    Time = (Get-Date).ToString("HH:mm:ss.fff")
    Id   = $p.Id
  }
  foreach ($f in $fields) {
    $v = $p.$f
    if ($null -eq $v) { $v = 0 }
    $o[$f] = $v
  }
  $o["ThreadCount"] = $p.Threads.Count
  return [PSCustomObject]$o
}

$proc = Get-Process -Name $ProcName -ErrorAction SilentlyContinue
if (-not $proc) {
  Write-Host "[mem-probe] process '$ProcName' is not running."
  Write-Host "[mem-probe] start it first, e.g.: target\release\pinshot.exe --debug"
  exit 1
}

$targetPid = $proc.Id
Write-Host "[mem-probe] sampling PID=$targetPid for ${Seconds}s every ${IntervalMs}ms"

$rows = New-Object System.Collections.Generic.List[object]
$deadline = (Get-Date).AddSeconds($Seconds)

while ((Get-Date) -lt $deadline) {
  $proc = Get-Process -Id $targetPid -ErrorAction SilentlyContinue
  if (-not $proc) {
    Write-Host "[mem-probe] process exited after $($rows.Count) samples"
    break
  }
  $rows.Add((Get-Snap $proc))
  Start-Sleep -Milliseconds $IntervalMs
}

if ($rows.Count -eq 0) { Write-Host "[mem-probe] no data"; exit 1 }

$ws = $rows | ForEach-Object { $_.WorkingSet64 }
$pm = $rows | ForEach-Object { $_.PrivateMemorySize64 }
$hd = $rows | ForEach-Object { $_.HandleCount }
$th = $rows | ForEach-Object { $_.ThreadCount }

$wsMin = ($ws | Measure-Object -Minimum).Minimum
$wsMax = ($ws | Measure-Object -Maximum).Maximum
$pmMin = ($pm | Measure-Object -Minimum).Minimum
$pmMax = ($pm | Measure-Object -Maximum).Maximum
$hdMin = ($hd | Measure-Object -Minimum).Minimum
$hdMax = ($hd | Measure-Object -Maximum).Maximum
$thMax = ($th | Measure-Object -Maximum).Maximum

$first = $rows[0]
$last  = $rows[-1]
$net   = $last.PrivateMemorySize64 - $first.PrivateMemorySize64

Write-Host ""
Write-Host "=== PID $targetPid : $($rows.Count) samples ==="
Write-Host ("Private    start {0,8:N1} MB   end {1,8:N1} MB   peak {2,8:N1} MB   min {3,8:N1} MB   net {4,8:N1} MB" -f `
  ($first.PrivateMemorySize64/1MB), ($last.PrivateMemorySize64/1MB), ($pmMax/1MB), ($pmMin/1MB), ($net/1MB))
Write-Host ("WorkingSet start {0,8:N1} MB   end {1,8:N1} MB   peak {2,8:N1} MB   min {3,8:N1} MB" -f `
  ($first.WorkingSet64/1MB), ($last.WorkingSet64/1MB), ($wsMax/1MB), ($wsMin/1MB))
Write-Host ("Handles    start {0,8}      end {1,8}      peak {2,8}      min {3,8}" -f `
  $first.HandleCount, $last.HandleCount, $hdMax, $hdMin)
Write-Host ("Threads    start {0,8}      end {1,8}      peak {2,8}" -f `
  $first.ThreadCount, $last.ThreadCount, $thMax)

if ($net -gt 20MB) {
  Write-Host ""
  Write-Host "[!] Private net +$([math]::Round($net/1MB,1)) MB - suspected leak or cache retention."
} elseif ($net -lt -5MB) {
  Write-Host ""
  Write-Host "[ok] Private dropped $([math]::Round(-$net/1MB,1)) MB - normal release."
} else {
  Write-Host ""
  Write-Host "[ok] Private stable (net $([math]::Round($net/1MB,1)) MB)."
}

if ($Out -ne "") {
  $rows | Export-Csv -Path $Out -NoTypeInformation -Encoding UTF8
  Write-Host ""
  Write-Host "[mem-probe] CSV written: $Out"
}

if (-not $Quiet) {
  Write-Host ""
  Write-Host "=== timeline (each line = 1 sample) ==="
  $scale = [math]::Max(1.0, ($pmMax / 60.0))
  foreach ($r in $rows) {
    $bars = [int]($r.PrivateMemorySize64 / 1MB / $scale)
    if ($bars -lt 1) { $bars = 1 }
    $line = "[{0}] {1,7:N1}MB {2}" -f $r.Time, ($r.PrivateMemorySize64/1MB), ("#" * $bars)
    if ($r.PrivateMemorySize64 -eq $pmMax) { $line += "  <- peak" }
    Write-Host $line
  }
}
