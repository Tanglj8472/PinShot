//==============================================================================
// SnipClone — DPI / multi-monitor coordinate helpers
//
// Inspired by PowerToys ZoomIt Utility.cpp (MIT License, Microsoft /
// Sysinternals). Rewritten for SnipClone; keep original copyright notices.
//==============================================================================
#pragma once

#include "WinCompat.h"

namespace snip
{

constexpr UINT kDpiBaseline = USER_DEFAULT_SCREEN_DPI; // 96

// Dynamically resolve GetDpiForWindow (Win10+); fall back to GetDeviceCaps.
UINT GetDpiForWindowHelper(HWND window);

// Monitor under cursor → full monitor rect in screen coordinates.
RECT GetMonitorRectFromCursor();

// Virtual desktop spanning all monitors (may have negative origin).
RECT GetVirtualScreenRect();

// Work area (excludes taskbar) for the monitor nearest to `rect`.
RECT GetMonitorWorkAreaFromRect(const RECT& rect);

// DPI of the monitor nearest to `rect` (Win8.1+ GetDpiForMonitor when available).
UINT GetDpiForRect(const RECT& rect);

int ScaleForDpi(int value, UINT dpi);

// Normalize two points into a RECT with optional minimum size (inclusive).
RECT RectFromPointsMinSize(POINT a, POINT b, LONG minSize);

// Clamp rect into bounds (both client or screen space).
RECT ForceRectInBounds(const RECT& rect, const RECT& bounds);

// True when rect intersects bounds (virtual-desktop / monitor math).
bool RectsIntersect(const RECT& a, const RECT& b);

} // namespace snip

