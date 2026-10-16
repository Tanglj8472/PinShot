param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$PortableZip,
    [Parameter(Mandatory = $true)][string]$Checksums,
    [string]$OutputDir = (Join-Path ([IO.Path]::GetTempPath()) ('pinshot-sandbox-' + [guid]::NewGuid().ToString('N')))
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (Test-Path -LiteralPath $OutputDir) { throw 'OutputDir must be a new directory' }
$sources = @($Installer, $PortableZip, $Checksums, (Join-Path $PSScriptRoot 'test-windows-sandbox-guest.ps1'))
foreach ($source in $sources) { if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing input: $source" } }
New-Item -ItemType Directory -Path $OutputDir | Out-Null
$root = (Resolve-Path -LiteralPath $OutputDir).Path
$inputDir = Join-Path $root 'input'
$results = Join-Path $root 'results'
New-Item -ItemType Directory -Path $inputDir, $results | Out-Null
foreach ($source in $sources) { Copy-Item -LiteralPath $source -Destination $inputDir }
$escape = { param($value) [Security.SecurityElement]::Escape($value) }
$xml = @"
<Configuration>
  <VGpu>Enable</VGpu>
  <Networking>Enable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$(& $escape $inputDir)</HostFolder>
      <SandboxFolder>C:\PinShotValidation\Input</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$(& $escape $results)</HostFolder>
      <SandboxFolder>C:\PinShotValidation\Results</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\PinShotValidation\Input\test-windows-sandbox-guest.ps1</Command>
  </LogonCommand>
</Configuration>
"@
$config = Join-Path $root 'PinShot-validation.wsb'
$xml | Set-Content -LiteralPath $config -Encoding utf8
Write-Host "Sandbox configuration: $config"
Write-Host "Reports: $results"
Write-Host 'Open the .wsb file on a host with Windows Sandbox enabled. No host feature is changed by this script.'
