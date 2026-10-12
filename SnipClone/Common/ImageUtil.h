//==============================================================================
// SnipClone — WIC PNG encode / clipboard write
//
// Save + clipboard helpers inspired by ZoomIt ImageEncoder / clipboard paths
// (MIT). Rewritten; uses WIC instead of GDI+ for PNG.
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"

#include <Windows.h>

#include <string>

namespace snip
{

class ImageUtil
{
public:
    // Write BGRA capture to PNG via WIC.
    static bool SavePng(const CaptureResult& image, const std::wstring& path);

    // Place CF_DIB on the clipboard (BGR bottom-up DIB; converts from BGRA).
    static bool CopyToClipboard(const CaptureResult& image);

    // Optional file dialog; returns empty if cancelled.
    static std::wstring BrowseSavePng(HWND owner);
};

} // namespace snip
