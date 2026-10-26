param(
    [Parameter(Mandatory = $true)][string]$PayloadDir,
    [Parameter(Mandatory = $true)][string]$Dumpbin,
    [switch]$Lite
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Only Windows components may be resolved outside the payload. In particular,
# an MSVC/ONNX/GPU runtime installed on the build machine must not hide an omission.
$systemDlls = @(
    'advapi32.dll', 'avrt.dll', 'bcrypt.dll', 'bcryptprimitives.dll',
    'cfgmgr32.dll', 'combase.dll', 'comctl32.dll', 'comdlg32.dll', 'crypt32.dll',
    'd3d11.dll', 'd3d12.dll', 'dbghelp.dll', 'dnsapi.dll', 'dwmapi.dll',
    'dxcore.dll', 'dxgi.dll', 'gdi32.dll', 'imm32.dll', 'iphlpapi.dll',
    'kernel32.dll', 'kernelbase.dll', 'msimg32.dll', 'msvcrt.dll', 'ncrypt.dll',
    'normaliz.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'opengl32.dll',
    'powrprof.dll', 'propsys.dll', 'psapi.dll', 'rpcrt4.dll', 'secur32.dll',
    'setupapi.dll', 'shell32.dll', 'shlwapi.dll', 'ucrtbase.dll', 'user32.dll',
    'userenv.dll', 'usp10.dll', 'uxtheme.dll', 'version.dll', 'winhttp.dll',
    'wininet.dll', 'winmm.dll', 'winspool.drv', 'wintrust.dll', 'ws2_32.dll',
    'wtsapi32.dll'
)
$files = @(Get-ChildItem -LiteralPath $PayloadDir -File | Where-Object { $_.Extension -in '.exe', '.dll' })
if (-not ($files | Where-Object Name -eq 'pinshot.exe')) { throw 'Payload has no pinshot.exe' }
if ($Lite -and ($files | Where-Object { $_.Name -match '(?i)onnx|directml|dxcore|dawn|cuda|cudnn' })) { throw 'Lite package contains an inference/GPU runtime' }
$localNames = @($files | ForEach-Object { $_.Name.ToLowerInvariant() })
$report = foreach ($file in $files) {
    $headers = & $Dumpbin /NOLOGO /HEADERS $file.FullName 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $headers -notmatch '8664 machine') {
        throw "Not an x64 PE binary: $($file.Name)"
    }
    $imports = & $Dumpbin /NOLOGO /DEPENDENTS $file.FullName 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect imports: $($file.Name)" }
    $dependencies = @([regex]::Matches($imports, '(?im)^\s+([a-z0-9_.-]+\.(?:dll|drv))\s*$') |
        ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() } | Sort-Object -Unique)
    foreach ($dependency in $dependencies) {
        if ($Lite -and $dependency -match '(?i)onnx|directml|dxcore|dawn|cuda|cudnn') { throw "Lite imports an unsupported runtime: $($file.Name) -> $dependency" }
        if ($dependency -in $localNames) { $source = 'bundled' }
        elseif ($dependency -in $systemDlls -or $dependency -match '^(api|ext)-ms-win-') { $source = 'Windows' }
        else { throw "Missing non-system dependency: $($file.Name) -> $dependency" }
        "$($file.Name) -> $dependency [$source]"
    }
}
$report | Set-Content -LiteralPath (Join-Path $PayloadDir 'runtime-dependencies.txt') -Encoding utf8
Write-Host "PASS: $($files.Count) x64 binaries; all imported dependencies are bundled or Windows components"
