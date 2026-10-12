//==============================================================================
// SnipClone — DPI / multi-monitor coordinate helpers
//
// Inspired by PowerToys ZoomIt Utility.cpp (MIT License, Microsoft /
// Sysinternals). Rewritten for SnipClone; keep original copyright notices.
//==============================================================================
#include "DpiHelper.h"

#include <algorithm>

namespace snip
{

UINT GetDpiForWindowHelper(HWND window)
{
    using GetDpiForWindowFn = UINT(WINAPI*)(HWND);
    static auto fn = reinterpret_cast<GetDpiForWindowFn>(
        GetProcAddress(GetModuleHandleW(L"user32.dll"), "GetDpiForWindow"));

    if (fn && window)
    {
        return fn(window);
    }

    HDC hdc = GetDC(nullptr);
    if (!hdc)
    {
        return kDpiBaseline;
    }
    const UINT dpi = static_cast<UINT>(GetDeviceCaps(hdc, LOGPIXELSX));
    ReleaseDC(nullptr, hdc);
    return dpi ? dpi : kDpiBaseline;
}

RECT GetMonitorRectFromCursor()
{
    POINT point{};
    GetCursorPos(&point);

    MONITORINFO info{};
    info.cbSize = sizeof(info);
    HMONITOR monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    if (monitor && GetMonitorInfoW(monitor, &info))
    {
        return info.rcMonitor;
    }
    return GetVirtualScreenRect();
}

RECT GetVirtualScreenRect()
{
    RECT rect{};
    rect.left = GetSystemMetrics(SM_XVIRTUALSCREEN);
    rect.top = GetSystemMetrics(SM_YVIRTUALSCREEN);
    rect.right = rect.left + GetSystemMetrics(SM_CXVIRTUALSCREEN);
    rect.bottom = rect.top + GetSystemMetrics(SM_CYVIRTUALSCREEN);
    return rect;
}

RECT GetMonitorWorkAreaFromRect(const RECT& rect)
{
    MONITORINFO info{};
    info.cbSize = sizeof(info);
    HMONITOR monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
    if (monitor && GetMonitorInfoW(monitor, &info))
    {
        return info.rcWork;
    }
    return GetVirtualScreenRect();
}

UINT GetDpiForRect(const RECT& rect)
{
    HMONITOR monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
    using GetDpiForMonitorFn = HRESULT(WINAPI*)(HMONITOR, int, UINT*, UINT*);
    static auto fn = reinterpret_cast<GetDpiForMonitorFn>(
        GetProcAddress(LoadLibraryW(L"shcore.dll"), "GetDpiForMonitor"));
    if (fn && monitor)
    {
        UINT dx = 0;
        UINT dy = 0;
        // MDT_EFFECTIVE_DPI = 0
        if (SUCCEEDED(fn(monitor, 0, &dx, &dy)) && dx != 0)
        {
            return dx;
        }
    }
    return GetDpiForWindowHelper(nullptr);
}

int ScaleForDpi(int value, UINT dpi)
{
    return MulDiv(value, static_cast<int>(dpi), static_cast<int>(kDpiBaseline));
}

RECT RectFromPointsMinSize(POINT a, POINT b, LONG minSize)
{
    RECT rect{};
    if (a.x <= b.x)
    {
        rect.left = a.x;
        rect.right = b.x + 1;
        if ((rect.right - rect.left) < minSize)
        {
            rect.right = rect.left + minSize;
        }
    }
    else
    {
        rect.left = b.x;
        rect.right = a.x + 1;
        if ((rect.right - rect.left) < minSize)
        {
            rect.left = rect.right - minSize;
        }
    }

    if (a.y <= b.y)
    {
        rect.top = a.y;
        rect.bottom = b.y + 1;
        if ((rect.bottom - rect.top) < minSize)
        {
            rect.bottom = rect.top + minSize;
        }
    }
    else
    {
        rect.top = b.y;
        rect.bottom = a.y + 1;
        if ((rect.bottom - rect.top) < minSize)
        {
            rect.top = rect.bottom - minSize;
        }
    }
    return rect;
}

RECT ForceRectInBounds(const RECT& rect, const RECT& bounds)
{
    RECT out = rect;
    const LONG width = out.right - out.left;
    const LONG height = out.bottom - out.top;

    if (out.left < bounds.left)
    {
        out.left = bounds.left;
        out.right = out.left + width;
    }
    if (out.top < bounds.top)
    {
        out.top = bounds.top;
        out.bottom = out.top + height;
    }
    if (out.right > bounds.right)
    {
        out.right = bounds.right;
        out.left = out.right - width;
    }
    if (out.bottom > bounds.bottom)
    {
        out.bottom = bounds.bottom;
        out.top = out.bottom - height;
    }

    out.left = (std::max)(out.left, bounds.left);
    out.top = (std::max)(out.top, bounds.top);
    out.right = (std::min)(out.right, bounds.right);
    out.bottom = (std::min)(out.bottom, bounds.bottom);
    return out;
}

bool RectsIntersect(const RECT& a, const RECT& b)
{
    return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}

} // namespace snip

