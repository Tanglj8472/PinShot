//==============================================================================
// SnipClone — region screen capture (GDI BitBlt)
//
// BitBlt-from-screen pattern inspired by PowerToys ZoomIt Utility /
// CreateGdiplusBitmap helpers (MIT). Rewritten for region-first snip.
//==============================================================================
#include "ScreenCapture.h"

#include <cstring>

namespace snip
{
namespace
{
int AlignStride(int widthPixels)
{
    // 32bpp → width*4 already 4-byte aligned; keep explicit for clarity.
    return widthPixels * 4;
}
} // namespace

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
    // SRCCOPY from virtual-desktop coordinates (multi-monitor safe with SM_*VIRTUALSCREEN origin).
    BitBlt(memDc, 0, 0, width, height, screenDc, screenRect.left, screenRect.top, SRCCOPY);

    out.width = width;
    out.height = height;
    out.stride = AlignStride(width);
    out.bgra.resize(static_cast<size_t>(out.stride) * static_cast<size_t>(height));
    memcpy(out.bgra.data(), bits, out.bgra.size());

    // Force opaque alpha — BitBlt often leaves alpha=0 which breaks layered/D2D previews.
    for (int y = 0; y < height; ++y)
    {
        auto* row = reinterpret_cast<std::uint32_t*>(out.bgra.data() + static_cast<size_t>(y) * out.stride);
        for (int x = 0; x < width; ++x)
        {
            row[x] |= 0xFF000000u;
        }
    }

    SelectObject(memDc, old);
    DeleteObject(dib);
    DeleteDC(memDc);
    ReleaseDC(nullptr, screenDc);
    return out;
}

} // namespace snip
