param(
    [Parameter(Mandatory=$true)][string]$DistDir,
    [string]$CertificateBase64,
    [string]$CertificatePassword,
    [string]$TimestampUrl = 'http://timestamp.digicert.com'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$DistDir = (Resolve-Path -LiteralPath $DistDir).Path
$setup = Get-ChildItem -LiteralPath $DistDir -Filter '*-setup.exe' -File | Select-Object -First 1
if (-not $setup) { throw 'No installer found to sign' }
$signtool = Get-ChildItem -LiteralPath (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin') -Filter signtool.exe -File -Recurse |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw 'Windows SDK signtool.exe not found' }
$pfx = Join-Path $env:TEMP ('pinshot-sign-' + [guid]::NewGuid().ToString('N') + '.pfx')
try {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($CertificateBase64))
    & $signtool.FullName sign /fd SHA256 /f $pfx /p $CertificatePassword /tr $TimestampUrl /td SHA256 $setup.FullName
    if ($LASTEXITCODE -ne 0) { throw 'signtool failed for installer' }
    Write-Host "Signed $($setup.Name)"
} finally {
    Remove-Item -LiteralPath $pfx -Force -ErrorAction SilentlyContinue
}
$sums = Get-ChildItem -LiteralPath $DistDir -File | Where-Object { $_.Name -match '(-setup\.exe|\.zip)$' } | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
}
$sums | Set-Content -LiteralPath (Join-Path $DistDir 'SHA256SUMS.txt') -Encoding ascii
