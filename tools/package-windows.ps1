param(
    [string]$Iscc,
    [switch]$SkipTests,
    [switch]$SkipBuild,
    [string]$ReleaseDir
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = Split-Path $PSScriptRoot -Parent
$workspace = Join-Path $repo 'rust'
$manifest = Get-Content -LiteralPath (Join-Path $workspace 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9.]+)"').Groups[1].Value
if (-not $version) { throw 'Workspace version missing' }
if (-not $Iscc) {
    $compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($compiler) { $Iscc = $compiler.Source }
    else { $Iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
}
if (-not (Test-Path -LiteralPath $Iscc)) { throw 'Install Inno Setup 6 or pass -Iscc <path>' }
$hostInfo = & rustc -vV
if ($LASTEXITCODE -ne 0 -or "$hostInfo" -notmatch 'host: x86_64-pc-windows-msvc') {
    throw 'Packaging requires the x86_64-pc-windows-msvc toolchain'
}
Push-Location $workspace
try {
    if (-not $SkipTests) {
        & cargo test --locked --workspace
        if ($LASTEXITCODE -ne 0) { throw 'Tests without OCR failed' }
        & cargo test --locked --workspace --features ocr
        if ($LASTEXITCODE -ne 0) { throw 'OCR tests failed' }
    }
    if (-not $SkipBuild) {
        & cargo build --locked --release --bin pinshot --features ocr
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    }
} finally { Pop-Location }

$dist = Join-Path $repo 'dist'
$stageRoot = Join-Path $dist ('stage-' + [guid]::NewGuid().ToString('N'))
$name = "PinShot-$version-windows-x64"
$payload = Join-Path $stageRoot $name
New-Item -ItemType Directory -Path $payload -Force | Out-Null
$release = if ($ReleaseDir) { $ReleaseDir } else { Join-Path $workspace 'target\release' }
Copy-Item -LiteralPath (Join-Path $release 'pinshot.exe') -Destination $payload
Get-ChildItem -LiteralPath $release -Filter '*.dll' -File | Copy-Item -Destination $payload
foreach ($dll in @('webgpu_dawn.dll', 'dxcompiler.dll', 'dxil.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $dll))) { throw "Missing OCR runtime: $dll" }
}
# App-local redistributable CRT: users do not need Visual Studio installed.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio Installer/vswhere is required' }
$vsRoot = & $vswhere -latest -products '*' -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vsRoot) { throw 'Visual Studio installation not found' }
$redistRoot = Join-Path $vsRoot 'VC\Redist\MSVC'
$crt = Get-ChildItem -LiteralPath $redistRoot -Directory | Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending | ForEach-Object {
        $x64 = Join-Path $_.FullName 'x64'
        if (Test-Path -LiteralPath $x64) { Get-ChildItem -LiteralPath $x64 -Directory -Filter 'Microsoft.VC*.CRT' }
    } | Select-Object -First 1
if (-not $crt) { throw 'MSVC x64 redistributable CRT not found' }
Get-ChildItem -LiteralPath $crt.FullName -File -Filter '*.dll' | Copy-Item -Destination $payload
foreach ($dll in @('msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $dll))) { throw "Missing CRT: $dll" }
}
Copy-Item -LiteralPath (Join-Path $repo 'assets\pinshot.ico') -Destination $payload
$readmeZh = Join-Path $repo 'README.zh-CN.md'
$readmeDefault = Join-Path $repo 'README.md'
$readmeSrc = if (Test-Path -LiteralPath $readmeZh) { $readmeZh } else { $readmeDefault }
Copy-Item -LiteralPath $readmeSrc -Destination (Join-Path $payload 'README.md')
Copy-Item -LiteralPath $readmeSrc -Destination (Join-Path $payload '使用说明.txt')
Copy-Item -LiteralPath (Join-Path $repo 'docs\third-party.md') -Destination $payload

# Include available upstream license files; no model or system font redistribution.
$licenses = Join-Path $payload 'licenses'
New-Item -ItemType Directory -Path $licenses | Out-Null
$cargoRoot = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$sources = @((Join-Path $cargoRoot 'registry\src'), (Join-Path $release 'build'))
$count = 0
foreach ($source in $sources) {
    if (-not (Test-Path -LiteralPath $source)) { continue }
    Get-ChildItem -LiteralPath $source -File -Recurse | Where-Object {
        $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|ThirdPartyNotices)([.-]|$)'
    } | ForEach-Object {
        $relative = $_.FullName.Substring($source.Length).TrimStart('\')
        $dest = Join-Path $licenses $relative
        New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
        $stream = [IO.File]::OpenRead($_.FullName)
        try {
            $header = New-Object byte[] 16
            $null = $stream.Read($header, 0, 16)
            $encrypted = [Text.Encoding]::ASCII.GetString($header).StartsWith('%TSD-Header-###%')
        } finally { $stream.Dispose() }
        if ($encrypted) {
            # Use the original Cargo archive when a local file-encryption driver
            # exposes ciphertext to packaging tools.
            $parts = $relative -split '\\', 3
            if ($parts.Count -ne 3) { throw "Encrypted license: $relative" }
            $archive = Join-Path $cargoRoot ("registry\cache\" + $parts[0] + '\' + $parts[1] + '.crate')
            if (-not (Test-Path -LiteralPath $archive)) { throw "Missing original license archive: $archive" }
            $member = $parts[1] + '/' + $parts[2].Replace('\', '/')
            $licenseText = & tar.exe -xOf $archive $member | Out-String
            if ($LASTEXITCODE -ne 0 -or -not $licenseText) { throw "License extraction failed: $member" }
            $licenseText | Set-Content -LiteralPath $dest -Encoding utf8
        } else {
            Copy-Item -LiteralPath $_.FullName -Destination $dest
        }
        $count++
    }
}
if ($count -eq 0) { throw 'No third-party licenses found' }
$reported = (& (Join-Path $payload 'pinshot.exe') --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or "$reported" -notmatch "PinShot $([regex]::Escape($version)) \(windows-rust\)") {
    throw "Version smoke failed: $reported"
}
& $Iscc "/DAppVersion=$version" "/DPayloadDir=$payload" "/DDistDir=$dist" (Join-Path $repo 'packaging\windows.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
$zip = Join-Path $dist "$name.zip"
Compress-Archive -LiteralPath $payload -DestinationPath $zip -Force
$artifacts = @((Join-Path $dist "$name-setup.exe"), $zip)
$sums = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $(Split-Path $artifact -Leaf)"
}
$sums | Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
Write-Host "Release artifacts: $dist"
