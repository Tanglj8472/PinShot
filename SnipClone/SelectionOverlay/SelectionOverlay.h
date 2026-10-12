//==============================================================================
// SnipClone — fullscreen selection overlay
//
// PinShot assembly: paint a frozen virtual-desktop capture, dim outside the
// selection (do NOT punch through to the live desktop), border + size label.
//
// Interaction drag/ESC inspired by PowerToys ZoomIt SelectRectangle (MIT).
//==============================================================================
#pragma once

#include "SelectionModel.h"

#include "../Capture/CaptureResult.h"
#include "../Common/WinCompat.h"

#include <d2d1.h>
#include <dwrite.h>

#include <functional>

namespace snip
{

// Posted to owner when overlay closes. wParam: 1=confirmed, 0=cancelled.
inline constexpr UINT WM_SNIP_SELECTION_DONE = WM_APP + 0x51;

class SelectionOverlay
{
public:
    using DoneCallback = std::function<void(bool confirmed, const SelectionModel& model)>;

    SelectionOverlay();
    ~SelectionOverlay();

    SelectionOverlay(const SelectionOverlay&) = delete;
    SelectionOverlay& operator=(const SelectionOverlay&) = delete;

    // freezeFrame: full virtual-desktop capture taken BEFORE this overlay shows.
    bool Start(HINSTANCE instance, HWND owner, CaptureResult freezeFrame, DoneCallback callback = {});
    void Stop(bool confirmed);

    HWND Window() const { return m_hwnd; }
    bool IsActive() const { return m_hwnd != nullptr; }
    const SelectionModel& Model() const { return m_model; }

    static const SelectionModel& LastSelection();
    // Crop of the freeze frame for the last confirmed selection.
    static const CaptureResult& LastCrop();

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    bool EnsureRenderResources(int width, int height);
    bool EnsureFreezeBitmap();
    void ReleaseSurfaceResources();
    void ReleaseAllResources();
    void Present();
    void RenderToBitmap();

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    DoneCallback m_callback;

    bool m_dragging = false;
    POINT m_ptStart{};
    POINT m_ptCurrent{};
    RECT m_clientBounds{};
    RECT m_screenOrigin{};
    UINT m_dpi = 96;
    SelectionModel m_model;

    CaptureResult m_freeze;
    ID2D1Bitmap* m_freezeBmp = nullptr;

    ID2D1Factory* m_d2dFactory = nullptr;
    ID2D1DCRenderTarget* m_rt = nullptr;
    ID2D1SolidColorBrush* m_brushWhite = nullptr;
    ID2D1SolidColorBrush* m_brushDim = nullptr;
    IDWriteFactory* m_dwrite = nullptr;
    IDWriteTextFormat* m_textFormat = nullptr;

    HDC m_screenDc = nullptr;
    HDC m_memDc = nullptr;
    HBITMAP m_dib = nullptr;
    HGDIOBJ m_oldBitmap = nullptr;
    void* m_bits = nullptr;
    int m_bitmapW = 0;
    int m_bitmapH = 0;

    static SelectionModel s_lastSelection;
    static CaptureResult s_lastCrop;
};

} // namespace snip
