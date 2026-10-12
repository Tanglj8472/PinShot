//==============================================================================
// SnipClone — pin window（Layered 置顶 + WM_NCHITTEST 拖拽）
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"

#include <Windows.h>
#include <d2d1.h>

namespace snip
{

class PinWindow
{
public:
    PinWindow() = default;
    ~PinWindow();

    bool Create(HINSTANCE instance, CaptureResult image, POINT screenOrigin);
    void Destroy();
    HWND Window() const { return m_hwnd; }

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    void Present();
    bool EnsureSurface();
    void ReleaseSurface();

    HWND m_hwnd = nullptr;
    CaptureResult m_image;
    POINT m_origin{};

    ID2D1Factory* m_factory = nullptr;
    ID2D1DCRenderTarget* m_rt = nullptr;
    ID2D1Bitmap* m_bmp = nullptr;
    HDC m_screenDc = nullptr;
    HDC m_memDc = nullptr;
    HBITMAP m_dib = nullptr;
    HGDIOBJ m_oldBmp = nullptr;
    void* m_bits = nullptr;
    int m_w = 0;
    int m_h = 0;
};

} // namespace snip
