# Build PinShot.exe on Windows (PowerShell). Usage:  powershell -ExecutionPolicy Bypass -File build_windows.ps1
$ErrorActionPreference = "Stop"
Set-Location -Path $PSScriptRoot
$py = if ($env:PYTHON) { $env:PYTHON } else { "py" }
if (-not (Test-Path ".venv-win")) {
    if ($py -eq "py") { & py -3 -m venv .venv-win } else { & $py -m venv .venv-win }
}
$vpy = Join-Path ".venv-win" "Scripts\python.exe"
& $vpy -m pip install --upgrade pip
& $vpy -m pip install -r requirements-windows.txt
& $vpy -m py_compile pinshot_launcher.py
& $vpy tools\make_icon.py
& $vpy tests\test_platform.py
& $vpy -m PyInstaller --noconfirm --clean pinshot.spec
$env:PINSHOT_VERSION_OUT = "$PSScriptRoot\ver.txt"
Start-Process -FilePath "dist\PinShot\PinShot.exe" -ArgumentList "--version" -Wait
Get-Content ver.txt
Write-Host "Built: $PSScriptRoot\dist\PinShot\PinShot.exe"
