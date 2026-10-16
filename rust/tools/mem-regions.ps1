# mem-regions.ps1 - locate large committed regions in a live process.
# ASCII only (PS 5.1 parses this file as ANSI/GBK without a BOM).
param(
  [string]$ProcName = "pinshot",
  [int]$TopN = 25
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;

public class VqScanner {
    [StructLayout(LayoutKind.Sequential)]
    struct MEMORY_BASIC_INFORMATION {
        public IntPtr BaseAddress;
        public IntPtr AllocationBase;
        public uint AllocationProtect;
        public UIntPtr RegionSize;
        public uint State;
        public uint Protect;
        public uint Type;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern UIntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MEMORY_BASIC_INFORMATION mbi, IntPtr len);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr VirtualQuery(IntPtr addr, out MEMORY_BASIC_INFORMATION mbi, IntPtr len);
    [DllImport("kernel32.dll")]
    static extern void GetSystemInfo(out SYSTEM_INFO lpSystemInfo);

    [StructLayout(LayoutKind.Sequential)]
    struct SYSTEM_INFO {
        public ushort wProcessorArchitecture;
        public ushort wReserved;
        public uint dwPageSize;
        public IntPtr lpMinimumApplicationAddress;
        public IntPtr lpMaximumApplicationAddress;
        public IntPtr dwActiveProcessorMask;
        public uint dwNumberOfProcessors;
        public uint dwProcessorType;
        public uint dwAllocationGranularity;
        public ushort wProcessorLevel;
        public ushort wProcessorRevision;
    }

    const uint MEM_COMMIT = 0x1000;
    const uint MEM_RESERVE = 0x2000;

    public class Row {
        public string Base;
        public string AllocBase;
        public double MB;
        public string State;
        public string Type;
        public string Protect;
        public long Hits;
    }

    public static List<Row> Scan(int pid) {
        var rows = new List<Row>();
        IntPtr h = OpenProcess(0x0400 | 0x0010 | 0x1000, false, pid); // QUERY|VM_READ|VM_OPERATION
        if (h == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());

        SYSTEM_INFO si = new SYSTEM_INFO();
        GetSystemInfo(out si);
        int mbiSize = Marshal.SizeOf(typeof(MEMORY_BASIC_INFORMATION));

        IntPtr cur = IntPtr.Zero;
        long totalCommit = 0;
        while (true) {
            MEMORY_BASIC_INFORMATION m;
            UIntPtr prev = VirtualQueryEx(h, cur, out m, new IntPtr(mbiSize));
            if (prev == UIntPtr.Zero) break;
            if (m.RegionSize.ToUInt64() == 0) break;

            if (m.State == MEM_COMMIT) {
                long sz = (long)m.RegionSize.ToUInt64();
                totalCommit += sz;
                if (sz >= 4L * 1024 * 1024) {
                    Row r = new Row();
                    r.Base = "0x" + m.BaseAddress.ToInt64().ToString("X");
                    r.AllocBase = "0x" + m.AllocationBase.ToInt64().ToString("X");
                    r.MB = Math.Round(sz / 1048576.0, 2);
                    r.State = "COMMIT";
                    r.Type = DescribeType(m.Type);
                    r.Protect = DescribeProtect(m.Protect, m.AllocationProtect);
                    r.Hits = 1;
                    rows.Add(r);
                }
            }
            cur = new IntPtr(m.BaseAddress.ToInt64() + (long)m.RegionSize.ToUInt64());
        }
        CloseHandle(h);

        // merge adjacent rows sharing AllocBase
        var merged = new Dictionary<string, Row>();
        foreach (Row r in rows) {
            if (merged.ContainsKey(r.AllocBase)) {
                merged[r.AllocBase].MB += r.MB;
            } else {
                merged[r.AllocBase] = r;
            }
        }
        var outp = new List<Row>(merged.Values);
        outp.Sort(delegate(Row a, Row b) { return b.MB.CompareTo(a.MB); });
        Console.WriteLine("TOTAL_COMMIT_MB=" + Math.Round(totalCommit / 1048576.0, 1));
        return outp;
    }

    static string DescribeType(uint t) {
        switch (t) {
            case 0x20000: return "IMAGE";
            case 0x40000: return "MAPPED";
            case 0x10000: return "PRIVATE";
            case 0x1000000: return "MEMORY";
            default: return "TYPE_" + t.ToString("X");
        }
    }

    static string DescribeProtect(uint p, uint allocProtect) {
        uint b = p & 0xFF;
        string s;
        switch (b) {
            case 0x01: s = "NOACCESS"; break;
            case 0x02: s = "READONLY"; break;
            case 0x04: s = "READWRITE"; break;
            case 0x08: s = "WRITECOPY"; break;
            case 0x10: s = "EXECUTE"; break;
            case 0x20: s = "EXECUTE_READ"; break;
            case 0x40: s = "EXECUTE_READWRITE"; break;
            case 0x80: s = "EXECUTE_WRITECOPY"; break;
            default: s = "P_" + b.ToString("X"); break;
        }
        bool guard = (p & 0x100) != 0;
        return s + (guard ? "|GUARD" : "");
    }
}
"@

$p = Get-Process -Name $ProcName -ErrorAction SilentlyContinue
if (-not $p) {
  Write-Host "process '$ProcName' not running"
  exit 1
}

Write-Host "PID=$($p.Id) Name=$($p.ProcessName)"
Write-Host ("Private={0:N1}MB WS={1:N1}MB Virt={2:N1}MB Pagefile={3:N1}MB PeakPagefile={4:N1}MB Handles={5}" -f `
  ($p.PrivateMemorySize64/1MB), ($p.WorkingSet64/1MB), ($p.VirtualMemorySize64/1MB), `
  ($p.PagedMemorySize64/1MB), ($p.PeakPagedMemorySize64/1MB), $p.HandleCount)
Write-Host ""

$rows = [VqScanner]::Scan($p.Id)
Write-Host "=== committed regions >= 4MB (top $($rows.Count), merged by AllocBase) ==="
Write-Host ("{0,-18} {1,-18} {2,12} {3,-10} {4,-28}" -f "BaseAddress","AllocBase","MB","Type","Protect")
$i = 0
foreach ($r in $rows) {
  if ($i -ge $TopN) { break }
  Write-Host ("{0,-18} {1,-18} {2,12:N1} {3,-10} {4,-28}" -f $r.Base, $r.AllocBase, $r.MB, $r.Type, $r.Protect)
  $i++
}

$sum = 0.0
foreach ($r in $rows) { $sum += $r.MB }
Write-Host ""
Write-Host "Sum of listed large regions: $([math]::Round($sum,1)) MB"
