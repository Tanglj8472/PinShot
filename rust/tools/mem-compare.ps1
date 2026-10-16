# mem-compare.ps1 - compare Private bytes across processes to tell a real
# per-process cost apart from a machine-wide / driver-wide artifact.
# ASCII only (PS 5.1 parses this file as ANSI/GBK without a BOM).
param(
  [string[]]$Names = @("pinshot", "explorer", "powershell", " devenv", "vmtoolsd"),
  [int]$TopN = 12
)

$ErrorActionPreference = "SilentlyContinue"

Write-Host "=== machine ==="
$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
Write-Host ("Computer : {0}  Manufacturer={1}  Model={2}" -f $cs.Name, $cs.Manufacturer, $cs.Model)
Write-Host ("TotalRAM : {0:N1} GB   FreeRAM: {1:N1} GB" -f `
  ($cs.TotalPhysicalMemory/1GB), ($os.FreePhysicalMemory*1KB/1GB))
Write-Host ("GPU      : {0}" -f ((Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join " | "))
Write-Host ""

Write-Host "=== top processes by Private bytes ==="
$all = Get-Process | Where-Object { $_.PrivateMemorySize64 -gt 0 }
$top = $all | Sort-Object -Property PrivateMemorySize64 -Descending | Select-Object -First $TopN
Write-Host ("{0,-26} {1,7} {2,12} {3,12} {4,12}" -f "Name", "PID", "PrivateMB", "WorkingSet", "VirtualMB")
foreach ($p in $top) {
  Write-Host ("{0,-26} {1,7} {2,12:N1} {3,12:N1} {4,12:N1}" -f `
    $p.ProcessName, $p.Id, ($p.PrivateMemorySize64/1MB), ($p.WorkingSet64/1MB), ($p.VirtualMemorySize64/1MB))
}
Write-Host ""

Write-Host "=== named processes ==="
$want = @("pinshot", "explorer", "devenv", "vmtoolsd", "vmware", "pwsh", "powershell", "dwm", "System")
$found = $all | Where-Object { $want -contains $_.ProcessName }
Write-Host ("{0,-26} {1,7} {2,12} {3,12}" -f "Name", "PID", "PrivateMB", "WorkingSet")
foreach ($p in $found) {
  Write-Host ("{0,-26} {1,7} {2,12:N1} {3,12:N1}" -f `
    $p.ProcessName, $p.Id, ($p.PrivateMemorySize64/1MB), ($p.WorkingSet64/1MB))
}
