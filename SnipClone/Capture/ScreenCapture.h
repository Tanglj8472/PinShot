//==============================================================================
// SnipClone — region screen capture
//
// V1: GDI BitBlt fallback (ZoomIt-style screen DC copy).
// Later: Windows.Graphics.Capture preferred path.
//==============================================================================
#pragma once

#include "CaptureResult.h"

namespace snip
{

class ScreenCapture
{
public:
    // Capture an absolute virtual-desktop rectangle into a top-down BGRA buffer.
    static CaptureResult CaptureRegion(const RECT& screenRect);
};

} // namespace snip
