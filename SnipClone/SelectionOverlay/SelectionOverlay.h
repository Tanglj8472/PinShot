//==============================================================================
// SnipClone — fullscreen selection overlay on a frozen capture
//
// PinShot: paint freeze, refine selection with handles, Enter/✓ confirm.
// Magnifier + HEX readout while hovering.
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

inline constexpr UINT WM_SNIP_SELECTION_DONE = WM_APP + 0x51;

class SelectionOverlay
{
public:
    using DoneCallback = std::function<void(bool confirmed, const SelectionModel& model)>;

    SelectionOverlay();
    ~SelectionOverlay();

    SelectionOverlay(const SelectionOverlay&) = delete;
    SelectionOverlay& operator=(const SelectionOverlay&) = delete;

    bool Start(HINSTANCE instance, HWND owner, CaptureResult freezeFrame, DoneCallback callback = {});
    void Stop(bool confirmed);

    HWND Window() const { return m_hwnd; }
    bool IsActive() const { return m_hwnd != nullptr; }
    const SelectionModel& Model() const { return m_model; }

    static const SelectionModel& LastSelection();
    static const CaptureResult& LastCrop();

private:
    enum class Phase
    {
        Idle,
        Creating,
        Adjusting,
    };

    enum class HitPart
    {
        None,
        Body,
        N,
        S,
        E,
        W,
        NE,
        NW,
        SE,
        SW,
        Confirm,
        Cancel,
    };

    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    bool EnsureRenderResources(int width, int height);
    bool EnsureFreezeBitmap();
    void ReleaseSurfaceResources();
    void ReleaseAllResources();
    void Present();
    void RenderToBitmap();
    void UpdateCursor(HitPart hit);
    HitPart HitTest(POINT client) const;
    RECT LocalSelection() const;
    void SetSelectionFromLocal(const RECT& local);
    void ApplyHandleDrag(HitPart part, POINT client);
    void SamplePixel(POINT client, COLORREF* outBgr, bool* ok) const;

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    DoneCallback m_callback;

    Phase m_phase = Phase::Idle;
    HitPart m_dragPart = HitPart::None;
    POINT m_ptStart{};
    POINT m_ptCurrent{};
    RECT m_dragStartSel{};
    RECT m_clientBounds{};
    RECT m_screenOrigin{};
    UINT m_dpi = 96;
    SelectionModel m_model;
    POINT m_cursor{};
    bool m_cursorValid = false;

    CaptureResult m_freeze;
    ID2D1Bitmap* m_freezeBmp = nullptr;

    ID2D1Factory* m_d2dFactory = nullptr;
    ID2D1DCRenderTarget* m_rt = nullptr;
    ID2D1SolidColorBrush* m_brushWhite = nullptr;
    ID2D1SolidColorBrush* m_brushDim = nullptr;
    ID2D1SolidColorBrush* m_brushAccent = nullptr;
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
