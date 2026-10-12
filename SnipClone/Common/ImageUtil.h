//==============================================================================
// SnipClone — WIC PNG encode / clipboard write
//
// Save + clipboard helpers inspired by ZoomIt ImageEncoder / clipboard paths
// (MIT). Rewritten; uses WIC instead of GDI+ for PNG.
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"

#include "WinCompat.h"

#include <string>

namespace snip
{

class ImageUtil
{
public:
    // Write BGRA capture to PNG via WIC.
    static bool SavePng(const CaptureResult& image, const std::wstring& path);

    // Decode PNG/JPEG/BMP/etc. via WIC into top-down BGRA CaptureResult.
    // screenRect is placed at (0,0)-(w,h) — flat import has no virtual-desktop origin.
    static bool LoadImageFile(const std::wstring& path, CaptureResult& out);

    // Place CF_DIB on the clipboard (BGR bottom-up DIB; converts from BGRA).
    static bool CopyToClipboard(const CaptureResult& image);

    // Optional file dialog; returns empty if cancelled.
    // initialDir may be empty (system default).
    static std::wstring BrowseSavePng(HWND owner, const std::wstring& initialDir = {});
    static std::wstring BrowseOpenImage(HWND owner, const std::wstring& initialDir = {});
};

} // namespace snip
