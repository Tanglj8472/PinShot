# ocr-sweep.ps1 - sweep OCR memory settings without rebuilding.
#
# ASCII ONLY. Windows PowerShell 5.1 reads .ps1 as ANSI/GBK unless a BOM is
# present, and CJK bytes inside string literals break the parser.
#
# Each run: start pinshot with the env vars set, trigger OCR manually on the SAME
# clip, then press Enter to read the numbers from the log.
#
# Usage:
#   .\tools\ocr-sweep.ps1 -Side 2560 -Trim
#   .\tools\ocr-sweep.ps1 -Side 2560 -Gpu 0 -Trim
param(
  [int]$Side = 2560,
  [string]$Gpu = "",
  [switch]$Trim
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe  = Join-Path $root "target\release\pinshot.exe"
if (-not (Test-Path $exe)) {
  Write-Host "build first: cargo build --release --bin pinshot --features ocr"
  exit 1
}

$dataLog = Join-Path $env:APPDATA "PinShot\pinshot.log"

if ($Trim -and (Test-Path $dataLog)) {
  Remove-Item $dataLog -Force
  Write-Host "[sweep] log truncated"
}

Set-Item -Path "env:PINSHOT_OCR_MAX_SIDE" -Value $Side
if ($Gpu -ne "") {
  Set-Item -Path "env:PINSHOT_OCR_GPU" -Value $Gpu
} else {
  # Clear a value inherited from a previous run in this same shell.
  Remove-Item -Path "env:PINSHOT_OCR_GPU" -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "=============================================="
Write-Host (" PINSHOT_OCR_MAX_SIDE = {0}" -f $Side)
Write-Host (" PINSHOT_OCR_GPU     = {0}" -f $(if ($Gpu -ne "") { $Gpu } else { "(auto)" }))
Write-Host "=============================================="
Write-Host ""
Write-Host "Now, in the app:"
Write-Host "  1. F7 to snip  (use the SAME clip on every run)"
Write-Host "  2. right-click the pin -> Recognize text"
Write-Host ""
Write-Host "Press Enter here after OCR has finished."

$p = Start-Process -FilePath $exe -ArgumentList "--debug" -PassThru
Write-Host ("PID={0}" -f $p.Id)

$null = Read-Host "Press Enter after OCR finished"
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

if (-not (Test-Path $dataLog)) { Write-Host ("no log at {0}" -f $dataLog); exit 1 }
$lines = Get-Content $dataLog

Write-Host ""
Write-Host "=== OCR lines ==="
$lines | Select-String -Pattern "ocr using|ocr blocks|ocr done|ocr fail|ocr empty|mem ocr" | ForEach-Object { $_.Line }

$using = $lines | Select-String -Pattern "ocr using"            | Select-Object -Last 1
$blocks = $lines | Select-String -Pattern "ocr blocks"           | Select-Object -Last 1
$pk     = $lines | Select-String -Pattern "mem ocr:post-load-models" | Select-Object -Last 1
$post   = $lines | Select-String -Pattern "mem ocr:post-infer"   | Select-Object -Last 1
$done   = $lines | Select-String -Pattern "ocr done"              | Select-Object -Last 1
$fail   = $lines | Select-String -Pattern "ocr fail"              | Select-Object -Last 1

Write-Host ""
Write-Host "=== extracted ==="
if ($fail) {
  Write-Host "  !! OCR FAILED - see 'ocr fail' above."
  Write-Host "     The engine loaded but the run died; usually an EP/model mismatch."
}
if ($using) { Write-Host ("  model         : " + ($using.Line -replace '^\[pinshot\]\s*ocr using\s+(\S+).*$','$1')) }
if ($blocks) { Write-Host ("  blocks        : " + ($blocks.Line -replace '^\[pinshot\]\s*ocr blocks\s+([0-9]+).*$','$1')) }
if ($pk)     { Write-Host ("  after load    : " + ($pk.Line   -replace '^\[pinshot\]\s*','')) }
if ($post)   { Write-Host ("  after infer   : " + ($post.Line -replace '^\[pinshot\]\s*','')) }
if ($done)   { Write-Host ("  timing        : " + ($done.Line -replace '^\[pinshot\]\s*','')) }
