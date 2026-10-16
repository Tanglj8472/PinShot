//==============================================================================
// SnipClone — screen capture (GDI BitBlt, ZoomIt-style)
//
// PinShot / 参考工具 assembly: capture virtual desktop first (freeze), then
// select a region on that frozen frame. ZoomIt Draw mode similarly works from
// a screen-compatible DC buffer (BitBlt + CAPTUREBLT).
//==============================================================================
#pragma once

#include "CaptureResult.h"

namespace snip
{

class ScreenCapture
{
public:
    // Full virtual desktop (all monitors). Call before showing any overlay.
    static CaptureResult CaptureVirtualScreen();

    // Absolute virtual-desktop rectangle → top-down BGRA.
    static CaptureResult CaptureRegion(const RECT& screenRect);
};

} // namespace snip
