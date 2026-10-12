//==============================================================================
// SnipClone — GDI BitBlt capture (ZoomIt Utility / Zoomit.cpp pattern)
//
// Upstream ZoomIt uses CreateCompatibleDC + BitBlt(... SRCCOPY|CAPTUREBLT)
// into a 32bpp top-down DIB, then forces usable alpha for layered/D2D paths.
//==============================================================================
#include "ScreenCapture.h"

#include "../Common/DpiHelper.h"

#include <cstring>

namespace snip
{
namespace
{
int AlignStride(int widthPixels)
{
    return widthPixels * 4;
}

void ForceOpaqueAlpha(CaptureResult& out)
{
    // BitBlt / CAPTUREBLT often leaves alpha=0; layered ULW_ALPHA needs opaque.
    for (int y = 0; y < out.height; ++y)
    {
        auto* row = reinterpret_cast<std::uint32_t*>(
            out.bgra.data() + static_cast<size_t>(y) * out.stride);
        for (int x = 0; x < out.width; ++x)
        {
            row[x] |= 0xFF000000u;
        }
    }
}
} // namespace

CaptureResult CaptureResult::CropToScreenRect(const RECT& want) const
{
    CaptureResult out{};
    if (Empty())
    {
        return out;
    }

    RECT src = screenRect;
    RECT clipped = want;
    if (clipped.left < src.left)
    {
        clipped.left = src.left;
    }
    if (clipped.top < src.top)
    {
        clipped.top = src.top;
    }
    if (clipped.right > src.right)
    {
        clipped.right = src.right;
    }
    if (clipped.bottom > src.bottom)
    {
        clipped.bottom = src.bottom;
    }

    const int w = clipped.right - clipped.left;
    const int h = clipped.bottom - clipped.top;
    if (w <= 0 || h <= 0)
    {
        return out;
    }

    const int x0 = clipped.left - src.left;
    const int y0 = clipped.top - src.top;

    out.width = w;
    out.height = h;
    out.stride = AlignStride(w);
    out.screenRect = clipped;
    out.bgra.resize(static_cast<size_t>(out.stride) * static_cast<size_t>(h));

    for (int y = 0; y < h; ++y)
    {
        const auto* s = bgra.data() + static_cast<size_t>(y0 + y) * stride
            + static_cast<size_t>(x0) * 4;
        auto* d = out.bgra.data() + static_cast<size_t>(y) * out.stride;
        memcpy(d, s, static_cast<size_t>(w) * 4);
    }
    return out;
}

CaptureResult ScreenCapture::CaptureRegion(const RECT& screenRect)
{
    CaptureResult out{};
    out.screenRect = screenRect;

    const int width = screenRect.right - screenRect.left;
    const int height = screenRect.bottom - screenRect.top;
    if (width <= 0 || height <= 0)
    {
        return out;
    }

    HDC screenDc = GetDC(nullptr);
    if (!screenDc)
    {
        return out;
    }

    HDC memDc = CreateCompatibleDC(screenDc);
    if (!memDc)
    {
        ReleaseDC(nullptr, screenDc);
        return out;
    }

    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height; // top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    void* bits = nullptr;
    HBITMAP dib = CreateDIBSection(memDc, &bmi, DIB_RGB_COLORS, &bits, nullptr, 0);
    if (!dib || !bits)
    {
        DeleteDC(memDc);
        ReleaseDC(nullptr, screenDc);
        return out;
    }

    HGDIOBJ old = SelectObject(memDc, dib);
    // ZoomIt: SRCCOPY | CAPTUREBLT so layered desktop composition is included.
    const BOOL blitOk = BitBlt(
        memDc,
        0,
        0,
        width,
        height,
        screenDc,
        screenRect.left,
        screenRect.top,
        SRCCOPY | CAPTUREBLT);

    if (blitOk)
    {
        out.width = width;
        out.height = height;
        out.stride = AlignStride(width);
        out.bgra.resize(static_cast<size_t>(out.stride) * static_cast<size_t>(height));
        memcpy(out.bgra.data(), bits, out.bgra.size());
        ForceOpaqueAlpha(out);
    }
    else
    {
        out = {};
        out.screenRect = screenRect;
    }

    SelectObject(memDc, old);
    DeleteObject(dib);
    DeleteDC(memDc);
    ReleaseDC(nullptr, screenDc);
    return out;
}

CaptureResult ScreenCapture::CaptureVirtualScreen()
{
    return CaptureRegion(GetVirtualScreenRect());
}

} // namespace snip
