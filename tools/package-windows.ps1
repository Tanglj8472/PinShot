param(
    [string]$Iscc,
    [switch]$SkipTests
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
Push-Location $workspace
try {
    if (-not $SkipTests) {
        & cargo test --locked --workspace
        if ($LASTEXITCODE -ne 0) { throw 'Tests without OCR failed' }
        & cargo test --locked --workspace --features ocr
        if ($LASTEXITCODE -ne 0) { throw 'OCR tests failed' }
    }
    & cargo build --locked --release --bin pinshot --features ocr
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
} finally { Pop-Location }

$dist = Join-Path $repo 'dist'
$stageRoot = Join-Path $dist ('stage-' + [guid]::NewGuid().ToString('N'))
$name = "PinShot-$version-windows-x64"
$payload = Join-Path $stageRoot $name
New-Item -ItemType Directory -Path $payload -Force | Out-Null
$release = Join-Path $workspace 'target\release'
Copy-Item -LiteralPath (Join-Path $release 'pinshot.exe') -Destination $payload
Get-ChildItem -LiteralPath $release -Filter '*.dll' -File | Copy-Item -Destination $payload
foreach ($dll in @('webgpu_dawn.dll', 'dxcompiler.dll', 'dxil.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $dll))) { throw "Missing OCR runtime: $dll" }
}
Copy-Item -LiteralPath (Join-Path $repo 'assets\pinshot.ico') -Destination $payload
Copy-Item -LiteralPath (Join-Path $repo 'README.md') -Destination $payload
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
        Copy-Item -LiteralPath $_.FullName -Destination $dest
        $count++
    }
}
if ($count -eq 0) { throw 'No third-party licenses found' }
$reported = & (Join-Path $payload 'pinshot.exe') --version
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
