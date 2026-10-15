param(
    [Parameter(Mandatory = $true)][string]$PayloadDir,
    [Parameter(Mandatory = $true)][string]$Iscc,
    [string]$AppVersion,
    [switch]$RejectShutdown,
    [switch]$FreshInstall,
    [switch]$RestartAfterInstall,
    [switch]$UpdateHelper
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = Split-Path $PSScriptRoot -Parent
if (-not $AppVersion) {
    $manifest = Get-Content -LiteralPath (Join-Path $repo 'rust\Cargo.toml') -Raw
    $AppVersion = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9.]+)"').Groups[1].Value
}
$existingPids = @(Get-Process -Name pinshot -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$testRoot = Join-Path (Join-Path $repo 'dist') ('upgrade-test-' + [guid]::NewGuid().ToString('N'))
$appDir = Join-Path $testRoot 'app with spaces'
$packages = Join-Path $testRoot 'packages'
$data = Join-Path $testRoot 'user data'
New-Item -ItemType Directory -Path $packages, $data -Force | Out-Null
$oldConfigDir = $env:PINSHOT_CONFIG_DIR
$oldDataDir = $env:PINSHOT_DATA_DIR
$env:PINSHOT_CONFIG_DIR = $data
$env:PINSHOT_DATA_DIR = $data
$config = Join-Path $data 'config.toml'
@('schema_version = 3', 'language = "en"', 'hotkey_snip = "Ctrl+Alt+P"', 'run_at_login = false',
  '[translate]', 'provider = "edge"', 'api_key = "test-key-do-not-use"',
  '[update]', 'check_on_startup = false') | Set-Content -LiteralPath $config -Encoding ascii
$history = Join-Path $data 'history.png'
$model = Join-Path $data 'model.onnx'
[IO.File]::WriteAllBytes($history, [byte[]](1, 2, 3, 4))
[IO.File]::WriteAllBytes($model, [byte[]](5, 6, 7, 8))
$dataHashes = @{}
foreach ($file in @($config, $history, $model)) { $dataHashes[$file] = (Get-FileHash -LiteralPath $file).Hash }
function AssertPreserved {
    foreach ($file in $dataHashes.Keys) {
        if ((Get-FileHash -LiteralPath $file).Hash -ne $dataHashes[$file]) { throw "User data changed: $file" }
    }
    foreach ($existingPid in $existingPids) {
        if (-not (Get-Process -Id $existingPid -ErrorAction SilentlyContinue)) {
            throw "An unrelated PinShot process was stopped: $existingPid"
        }
    }
}
$process = $null
try {
    if ($UpdateHelper) {
        New-Item -ItemType Directory -Path $appDir | Out-Null
        Get-ChildItem -LiteralPath $PayloadDir -File -Filter '*.dll' | Copy-Item -Destination $appDir
        $exe = Join-Path $appDir 'pinshot.exe'
        Copy-Item -LiteralPath (Join-Path $PayloadDir 'pinshot.exe') -Destination $exe
        $setup = Join-Path $packages 'fake-setup.exe'
        & rustc --edition=2021 (Join-Path $repo 'packaging\tests\installer-args.rs') -o $setup
        if ($LASTEXITCODE -ne 0) { throw 'Fake installer compilation failed' }
        & $exe --install-after-quit $setup | Out-String | Write-Host
        if ($LASTEXITCODE -ne 0) { throw 'Auto-update helper failed' }
        $argsFile = [IO.Path]::ChangeExtension($setup, 'args.txt')
        for ($i = 0; $i -lt 100 -and -not (Test-Path -LiteralPath $argsFile); $i++) { Start-Sleep -Milliseconds 100 }
        $captured = @(Get-Content -LiteralPath $argsFile)
        foreach ($arg in @('/NORESTART', '/PINSHOTRESTART=1', "/DIR=$appDir")) {
            if ($captured -notcontains $arg) { throw "Missing update installer argument: $arg" }
        }
        AssertPreserved
        Write-Host 'PASS: auto-update helper preserves destination, requests restart, and leaves other installations/data intact'
        return
    }

    $appId = '{' + [guid]::NewGuid().ToString() + '}'
    $installerPayload = $PayloadDir
    if ($RejectShutdown -or $RestartAfterInstall) {
        $installerPayload = Join-Path $testRoot 'fixture-payload'
        Copy-Item -LiteralPath $PayloadDir -Destination $installerPayload -Recurse
        $fixtureCfg = if ($RejectShutdown) { 'fail_shutdown' } else { 'restart_app' }
        & rustc --edition=2021 --cfg $fixtureCfg (Join-Path $repo 'packaging\tests\locked-app.rs') -o (Join-Path $installerPayload 'pinshot.exe')
        if ($LASTEXITCODE -ne 0) { throw 'Shutdown/restart fixture compilation failed' }
    }
    & $Iscc "/DInstallerAppId={$appId" "/DAppVersion=$AppVersion" "/DPayloadDir=$installerPayload" "/DDistDir=$packages" (Join-Path $repo 'packaging\windows.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Test installer compilation failed' }
    $fixture = Join-Path $appDir 'pinshot.exe'
    $oldHash = ''
    if (-not $FreshInstall -and -not $RestartAfterInstall) {
        New-Item -ItemType Directory -Path $appDir | Out-Null
        Get-ChildItem -LiteralPath $PayloadDir -File -Filter '*.dll' | Copy-Item -Destination $appDir
        & rustc --edition=2021 (Join-Path $repo 'packaging\tests\locked-app.rs') -o $fixture
        if ($LASTEXITCODE -ne 0) { throw 'Locked app fixture compilation failed' }
        $process = Start-Process -FilePath $fixture -WorkingDirectory $appDir -WindowStyle Hidden -PassThru
        $oldHash = (Get-FileHash -LiteralPath $fixture).Hash
        $ready = Join-Path $appDir 'fixture-ready.txt'
        for ($i = 0; $i -lt 50 -and -not (Test-Path -LiteralPath $ready); $i++) {
            Start-Sleep -Milliseconds 100
            $process.Refresh()
            if ($process.HasExited) { throw 'Locked app fixture exited before upgrade' }
        }
        if (-not (Test-Path -LiteralPath $ready)) { throw 'Locked app fixture did not become ready' }
    }
    $installer = Join-Path $packages "PinShot-$AppVersion-windows-x64-setup.exe"
    $log = Join-Path $testRoot 'setup.log'
    $installArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/TASKS=', "/DIR=$appDir", "/LOG=$log")
    if ($RestartAfterInstall) { $installArgs += '/PINSHOTRESTART=1' }
    & $installer @installArgs | Out-String | Write-Host
    $setupExit = $LASTEXITCODE
    AssertPreserved
    if ($RejectShutdown) {
        $process.Refresh()
        if ($setupExit -eq 0 -or $process.HasExited) { throw 'Rejected install did not preserve the old running application' }
        if ((Get-FileHash -LiteralPath $fixture).Hash -ne $oldHash) { throw 'Rejected install replaced the executable' }
        if ((Get-Content -LiteralPath $log -Raw) -notmatch 'helper exit code 23') { throw "Shutdown error missing from log: $log" }
        Write-Host "PASS: shutdown failure stops installation and preserves files/config. Log: $log"
        return
    }
    if ($setupExit -ne 0) { throw "Installer failed: $setupExit. Log: $log" }
    if ($process) {
        $process.Refresh()
        if (-not $process.HasExited) { throw 'The old application is still running' }
    }
    if (Test-Path -LiteralPath (Join-Path $appDir 'old-helper-was-called.txt')) { throw 'Installer used the old application as its helper' }
    $expectedHash = (Get-FileHash -LiteralPath (Join-Path $installerPayload 'pinshot.exe')).Hash
    if ((Get-FileHash -LiteralPath $fixture).Hash -ne $expectedHash) { throw 'Installed executable was not replaced' }
    if ($RestartAfterInstall) {
        $pidFile = Join-Path $appDir 'fixture-pid.txt'
        for ($i = 0; $i -lt 100 -and -not (Test-Path -LiteralPath $pidFile); $i++) { Start-Sleep -Milliseconds 100 }
        $restartedPid = [int](Get-Content -LiteralPath $pidFile -Raw)
        $process = Get-Process -Id $restartedPid
        if ([IO.Path]::GetFullPath($process.Path) -ne [IO.Path]::GetFullPath($fixture)) { throw 'Wrong application restarted' }
        Write-Host 'PASS: silent update restarts the newly installed application'
    } else {
        $version = (& $fixture --version | Out-String).Trim()
        if ($version -notmatch "PinShot $([regex]::Escape($AppVersion))") { throw "Wrong installed version: $version" }
        Write-Host "PASS: install/upgrade succeeds with intact config/models/history. Log: $log"
    }
} finally {
    try {
        $uninstaller = Join-Path $appDir 'unins000.exe'
        if (Test-Path -LiteralPath $uninstaller) {
            & $uninstaller /VERYSILENT /SUPPRESSMSGBOXES /NORESTART | Out-String | Write-Host
            if ($LASTEXITCODE -ne 0) { throw 'Isolated uninstall failed' }
            if (Test-Path -LiteralPath (Join-Path $appDir 'pinshot.exe')) { throw 'Uninstall left application executable behind' }
            Write-Host 'PASS: uninstall removes program files and preserves user data'
        }
        AssertPreserved
    } finally {
        if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() } }
        $env:PINSHOT_CONFIG_DIR = $oldConfigDir
        $env:PINSHOT_DATA_DIR = $oldDataDir
    }
}
