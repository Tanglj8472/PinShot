#Requires -Version 7.0
<#
.SYNOPSIS
  Build SnipClone with VS2022 MSBuild (Windows only). Run LogicTests + --smoke.
#>
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',
    [ValidateSet('x64', 'Win32')]
    [string]$Platform = 'x64',
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

function Find-MSBuild {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        throw 'vswhere.exe not found. Install Visual Studio 2022 with C++ desktop workload.'
    }
    $msbuild = & $vswhere -latest -requires Microsoft.Component.MSBuild -find 'MSBuild\**\Bin\MSBuild.exe' |
        Select-Object -First 1
    if (-not $msbuild) {
        throw 'MSBuild.exe not found.'
    }
    return $msbuild
}

$msbuild = Find-MSBuild
Write-Host "MSBuild: $msbuild"
& $msbuild .\SnipClone.sln /m /p:Configuration=$Configuration /p:Platform=$Platform
if ($LASTEXITCODE -ne 0) {
    throw "Build failed with exit code $LASTEXITCODE"
}

$out = Join-Path $PSScriptRoot "bin\$Platform\$Configuration\SnipClone.exe"
Write-Host "Output: $out"

if (-not $SkipTests -and $Platform -eq 'x64') {
    $tests = Join-Path $PSScriptRoot "bin\$Platform\$Configuration\SnipClone.Tests.exe"
    if (-not (Test-Path $tests)) {
        throw "Tests binary missing: $tests"
    }
    Write-Host "Running $tests"
    & $tests
    if ($LASTEXITCODE -ne 0) {
        throw "LogicTests failed with exit code $LASTEXITCODE"
    }

    if (-not (Test-Path $out)) {
        throw "SnipClone.exe missing: $out"
    }
    Write-Host "Running smoke: $out --smoke"
    & $out --smoke
    if ($LASTEXITCODE -ne 0) {
        throw "SnipClone --smoke failed with exit code $LASTEXITCODE"
    }
    Write-Host "Smoke OK"
}
