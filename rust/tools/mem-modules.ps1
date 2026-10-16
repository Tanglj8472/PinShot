# mem-modules.ps1 - map committed regions to the module (DLL/EXE) that owns them.
# ASCII only (PS 5.1 parses this file as ANSI/GBK without a BOM).
param(
  [string]$ProcName = "pinshot",
  [int]$MinMB = 8
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public class ModMap {
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
    [DllImport("psapi.dll", SetLastError = true)]
    static extern bool EnumProcessModules(IntPtr h, IntPtr[] mods, uint size, out uint needed);
    [DllImport("psapi.dll", SetLastError = true)]
    static extern uint GetModuleFileNameEx(IntPtr h, IntPtr mod, System.Text.StringBuilder buf, uint size);
    [DllImport("psapi.dll", SetLastError = true)]
    static extern uint GetModuleBaseName(IntPtr h, IntPtr mod, System.Text.StringBuilder buf, uint size, uint flags);

    const uint MEM_COMMIT = 0x1000;

    public class Line {
        public string Mod;
        public string Kind;
        public double MB;
        public long Regions;
    }

    public static List<Line> Scan(int pid, long minBytes) {
        IntPtr h = OpenProcess(0x0400 | 0x0010 | 0x1000, false, pid);
        if (h == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());

        // module base -> name
        Dictionary<long, string> names = new Dictionary<long, string>();
        IntPtr[] mods = new IntPtr[4096];
        uint needed = 0;
        if (EnumProcessModules(h, mods, (uint)(mods.Length * IntPtr.Size), out needed)) {
            int n = (int)(needed / IntPtr.Size);
            for (int i = 0; i < n && i < mods.Length; i++) {
                System.Text.StringBuilder sb = new System.Text.StringBuilder(1024);
                uint len = GetModuleFileNameEx(h, mods[i], sb, (uint)sb.Capacity);
                string full = len > 0 ? sb.ToString() : null;
                System.Text.StringBuilder nb = new System.Text.StringBuilder(512);
                GetModuleBaseName(h, mods[i], nb, (uint)nb.Capacity, 0);
                string baseName = nb.ToString();
                names[mods[i].ToInt64()] = baseName + "|" + (full != null ? System.IO.Path.GetFileName(full) : "?");
            }
        }

        Dictionary<string, Line> agg = new Dictionary<string, Line>();
        int mbiSize = Marshal.SizeOf(typeof(MEMORY_BASIC_INFORMATION));
        IntPtr cur = IntPtr.Zero;
        while (true) {
            MEMORY_BASIC_INFORMATION m;
            UIntPtr prev = VirtualQueryEx(h, cur, out m, new IntPtr(mbiSize));
            if (prev == UIntPtr.Zero) break;
            if (m.RegionSize.ToUInt64() == 0) break;
            if (m.State == MEM_COMMIT) {
                long sz = (long)m.RegionSize.ToUInt64();
                if (sz >= minBytes) {
                    long ab = m.AllocationBase.ToInt64();
                    string key;
                    if (names.ContainsKey(ab)) {
                        key = names[ab];
                    } else if (m.Type == 0x10000) {
                        key = "<private heap / unnamed>";
                    } else if (m.Type == 0x20000) {
                        key = "<image, no module match>";
                    } else if (m.Type == 0x40000) {
                        key = "<mapped file, no module match>";
                    } else {
                        key = "<type 0x" + m.Type.ToString("X") + ">";
                    }
                    if (!agg.ContainsKey(key)) {
                        Line l = new Line();
                        l.Mod = key; l.MB = 0; l.Regions = 0;
                        agg[key] = l;
                    }
                    agg[key].MB += sz / 1048576.0;
                    agg[key].Regions += 1;
                }
            }
            cur = new IntPtr(m.BaseAddress.ToInt64() + (long)m.RegionSize.ToUInt64());
        }
        CloseHandle(h);

        var outp = new List<Line>(agg.Values);
        outp.Sort(delegate(Line a, Line b) { return b.MB.CompareTo(a.MB); });
        return outp;
    }
}
"@

$p = Get-Process -Name $ProcName -ErrorAction SilentlyContinue
if (-not $p) { Write-Host "process '$ProcName' not running"; exit 1 }

Write-Host ("PID={0} Private={1:N1}MB WS={2:N1}MB" -f $p.Id, ($p.PrivateMemorySize64/1MB), ($p.WorkingSet64/1MB))
Write-Host ""

$lines = [ModMap]::Scan($p.Id, ([long]$MinMB * 1024 * 1024))
Write-Host "=== committed regions >= $MinMB MB, grouped by owning module ==="
foreach ($l in $lines) {
  Write-Host ("{0,10:N1} MB  {1,4} regions  {2}" -f $l.MB, $l.Regions, $l.Mod)
}
