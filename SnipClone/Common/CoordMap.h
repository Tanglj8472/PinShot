//==============================================================================
// SnipClone — virtual-desktop ↔ image-pixel coordinate map
//
// Contract (PerMonitorV2, physical pixels):
//   CaptureResult.screenRect  = absolute virtual-desktop rect of the buffer
//   image pixel (x, y)        = screen − screenRect.origin
//   screen may be negative (secondary monitor left/above primary)
//==============================================================================
#pragma once

#include "DpiHelper.h"
#include "WinCompat.h"

#include <d2d1.h>

namespace snip
{

inline POINT ImageOrigin(const RECT& imageScreenRect)
{
    return { imageScreenRect.left, imageScreenRect.top };
}

inline POINT ScreenToImage(POINT origin, POINT screen)
{
    return { screen.x - origin.x, screen.y - origin.y };
}

inline POINT ImageToScreen(POINT origin, POINT image)
{
    return { image.x + origin.x, image.y + origin.y };
}

inline POINT ScreenToImage(const RECT& imageScreenRect, POINT screen)
{
    return ScreenToImage(ImageOrigin(imageScreenRect), screen);
}

inline POINT ImageToScreen(const RECT& imageScreenRect, POINT image)
{
    return ImageToScreen(ImageOrigin(imageScreenRect), image);
}

inline D2D1_POINT_2F ScreenToImageF(const RECT& imageScreenRect, POINT screen)
{
    return D2D1::Point2F(
        static_cast<float>(screen.x - imageScreenRect.left),
        static_cast<float>(screen.y - imageScreenRect.top));
}

inline POINT ImageFToScreen(const RECT& imageScreenRect, D2D1_POINT_2F image)
{
    return {
        imageScreenRect.left + static_cast<LONG>(image.x),
        imageScreenRect.top + static_cast<LONG>(image.y),
    };
}

// Keep a top-left (x,y) + size inside the virtual desktop (negative origins OK).
inline void ClampTopLeftToVirtualScreen(int& x, int& y, int width, int height)
{
    const RECT vs = GetVirtualScreenRect();
    if (width > 0 && x + width > vs.right)
    {
        x = vs.right - width;
    }
    if (height > 0 && y + height > vs.bottom)
    {
        y = vs.bottom - height;
    }
    if (x < vs.left)
    {
        x = vs.left;
    }
    if (y < vs.top)
    {
        y = vs.top;
    }
}

} // namespace snip
