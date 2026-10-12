//==============================================================================
// SnipClone — window / layered-window helpers
//==============================================================================
#pragma once

#include "WinCompat.h"

namespace snip
{

// Register a window class if it does not already exist with the same WndProc.
bool EnsureWindowClass(const WNDCLASSEXW& wc);

// Virtual-screen sized topmost layered popup (no WS_EX_TRANSPARENT —
// SelectionOverlay must receive mouse input).
HWND CreateTopmostLayeredPopup(
    HWND owner,
    HINSTANCE instance,
    const wchar_t* className,
    void* createParam);

} // namespace snip
