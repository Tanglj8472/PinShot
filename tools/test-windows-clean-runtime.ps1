param(
    [Parameter(Mandatory = $true)][string]$PayloadDir,
    [string]$OutputDir = (Join-Path ([IO.Path]::GetTempPath()) ('pinshot-runtime-' + [guid]::NewGuid().ToString('N')))
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$payload = (Resolve-Path -LiteralPath $PayloadDir).Path
$exe = Join-Path $payload 'pinshot.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'pinshot.exe not found' }
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$output = (Resolve-Path -LiteralPath $OutputDir).Path
$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = $exe
$start.Arguments = '--settings-window'
$start.WorkingDirectory = $payload
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$start.EnvironmentVariables['PATH'] = "$env:SystemRoot\System32;$env:SystemRoot"
$start.EnvironmentVariables['PINSHOT_CONFIG_DIR'] = Join-Path $output 'config'
$start.EnvironmentVariables['PINSHOT_DATA_DIR'] = Join-Path $output 'data'
$start.EnvironmentVariables['PINSHOT_DEBUG'] = '0'
$process = [Diagnostics.Process]::Start($start)
Write-Host "Report and diagnostics: $output"
try {
    # Native event loops can repaint continuously without becoming input-idle.
    # Observe the actual window instead of using WaitForInputIdle as readiness.
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw "Settings exited during startup: $($process.ExitCode)" }
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    Start-Sleep -Milliseconds 1000
    $process.Refresh()
    if ($process.HasExited) { throw "Settings exited during startup: $($process.ExitCode)" }
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'Settings has no window' }
    $modules = @($process.Modules)
    foreach ($name in @('msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
        $module = $modules | Where-Object { $_.ModuleName -ieq $name } | Select-Object -First 1
        if (-not $module) { throw "Runtime not loaded: $name" }
        if ([IO.Path]::GetDirectoryName($module.FileName) -ine $payload) {
            throw "Runtime loaded outside the package: $name"
        }
    }
    $report = @(
        "OS: $([Environment]::OSVersion.VersionString)",
        "Executable: $exe",
        'PASS: settings window started with system-only PATH and isolated user data',
        'PASS: all four required MSVC runtime DLLs were loaded from the package',
        'This smoke test does not verify screenshot interactions or OCR model inference.'
    )
    $report | Set-Content -LiteralPath (Join-Path $output 'runtime-smoke.txt') -Encoding utf8
    $report | Write-Host
    Write-Host "Report and diagnostics: $output"
} catch {
    "FAIL: $($_.Exception.Message)" | Set-Content -LiteralPath (Join-Path $output 'runtime-smoke.txt') -Encoding utf8
    throw
} finally {
    $process.Refresh()
    if (-not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    $process.Dispose()
}
