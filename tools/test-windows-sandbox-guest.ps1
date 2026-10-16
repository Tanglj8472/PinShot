param(
    [string]$InputDir = 'C:\PinShotValidation\Input',
    [string]$ResultsDir = 'C:\PinShotValidation\Results'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
New-Item -ItemType Directory -Path $ResultsDir -Force | Out-Null
Start-Transcript -LiteralPath (Join-Path $ResultsDir 'sandbox-transcript.txt') | Out-Null
$app = $null
try {
    foreach ($line in Get-Content -LiteralPath (Join-Path $InputDir 'SHA256SUMS.txt')) {
        if ($line -notmatch '^([a-fA-F0-9]{64})\s+([^\\/]+)$') { throw 'Invalid checksum manifest entry' }
        $expected = $Matches[1]
        $file = Join-Path $InputDir $Matches[2]
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ine $expected) { throw "Hash mismatch: $file" }
    }
    $installers = @(Get-ChildItem -LiteralPath $InputDir -Filter '*-setup.exe' -File)
    $zips = @(Get-ChildItem -LiteralPath $InputDir -Filter '*.zip' -File)
    if ($installers.Count -ne 1 -or $zips.Count -ne 1) { throw 'Expected exactly one installer and portable ZIP' }
    $portableRoot = Join-Path ([IO.Path]::GetTempPath()) ('pinshot-portable-' + [guid]::NewGuid().ToString('N'))
    Expand-Archive -LiteralPath $zips[0].FullName -DestinationPath $portableRoot
    $payloads = @(Get-ChildItem -LiteralPath $portableRoot -Directory)
    if ($payloads.Count -ne 1) { throw 'Unexpected portable ZIP layout' }
    $portable = $payloads[0].FullName
    $smoke = Join-Path $portable 'test-windows-clean-runtime.ps1'
    & $smoke -PayloadDir $portable -OutputDir (Join-Path $ResultsDir 'portable')
    $installDir = Join-Path $env:LOCALAPPDATA 'PinShot Validation\app with spaces'
    function Run-Installer([string[]]$Arguments) {
        $process = Start-Process -FilePath $installers[0].FullName -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
    }
    Run-Installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/TASKS=',
        "/DIR=`"$installDir`"", "/LOG=`"$(Join-Path $ResultsDir 'install.log')`"")
    $installedExe = Join-Path $installDir 'pinshot.exe'
    $expectedExe = (Get-FileHash -LiteralPath (Join-Path $portable 'pinshot.exe')).Hash
    if ((Get-FileHash -LiteralPath $installedExe).Hash -ne $expectedExe) { throw 'Installed executable differs from portable ZIP' }
    & $smoke -PayloadDir $installDir -OutputDir (Join-Path $ResultsDir 'installed')
    $app = Start-Process -FilePath $installedExe -ArgumentList '--settings-window' -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 2
    $app.Refresh()
    if ($app.HasExited) { throw 'Settings exited before the upgrade test' }
    Run-Installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/TASKS=',
        "/LOG=`"$(Join-Path $ResultsDir 'upgrade.log')`"")
    $app.Refresh()
    if (-not $app.HasExited) { throw 'Upgrade did not close the running installed application' }
    if ((Get-FileHash -LiteralPath $installedExe).Hash -ne $expectedExe) { throw 'Upgrade did not retain the custom destination' }
    @(
        "OS: $([Environment]::OSVersion.VersionString)",
        'PASS: package hashes, portable and installed settings startup, app-local CRT loading',
        'PASS: custom directory installation and running-application upgrade without /DIR',
        'PENDING: manual screenshot, annotation, pinning, OCR download and offline OCR',
        "Installed program: $installedExe"
    ) | Set-Content -LiteralPath (Join-Path $ResultsDir 'sandbox-result.txt') -Encoding utf8
} catch {
    "FAIL: $($_.Exception.Message)" | Set-Content -LiteralPath (Join-Path $ResultsDir 'sandbox-result.txt') -Encoding utf8
    throw
} finally {
    if ($app) {
        $app.Refresh()
        if (-not $app.HasExited) { $app.Kill(); $app.WaitForExit() }
        $app.Dispose()
    }
    Stop-Transcript | Out-Null
}
