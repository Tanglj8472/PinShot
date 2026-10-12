//==============================================================================
// SnipClone — fullscreen selection overlay (V0)
//
// Snipaste-style: dim mask + clear hole + border + live pixel size.
// Does NOT capture the screen (capture lands in V1).
//
// Interaction pattern inspired by PowerToys ZoomIt SelectRectangle
// (MIT License, Microsoft / Sysinternals) — rewritten for region-first snip.
//==============================================================================
#pragma once

#include "SelectionModel.h"

#include <Windows.h>
#include <d2d1.h>
#include <dwrite.h>

#include <functional>

namespace snip
{

// Posted to owner when overlay closes. wParam: 1=confirmed, 0=cancelled.
// lParam: unused (read LastSelection() for rect).
inline constexpr UINT WM_SNIP_SELECTION_DONE = WM_APP + 0x51;

class SelectionOverlay
{
public:
    using DoneCallback = std::function<void(bool confirmed, const SelectionModel& model)>;

    SelectionOverlay();
    ~SelectionOverlay();

    SelectionOverlay(const SelectionOverlay&) = delete;
    SelectionOverlay& operator=(const SelectionOverlay&) = delete;

    // Create + show overlay. owner receives WM_SNIP_SELECTION_DONE on close.
    bool Start(HINSTANCE instance, HWND owner, DoneCallback callback = {});
    void Stop(bool confirmed);

    HWND Window() const { return m_hwnd; }
    bool IsActive() const { return m_hwnd != nullptr; }
    const SelectionModel& Model() const { return m_model; }

    // Last confirmed selection (survives overlay destroy until next Start).
    static const SelectionModel& LastSelection();

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    bool EnsureRenderResources(int width, int height);
    void ReleaseSurfaceResources(); // brushes + DC RT + DIB (keep factories)
    void ReleaseAllResources();     // everything including D2D/DWrite factories
    void Present();
    void RenderToBitmap();

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    DoneCallback m_callback;

    bool m_dragging = false;
    POINT m_ptStart{};   // client coords
    POINT m_ptCurrent{}; // client coords
    RECT m_clientBounds{};
    RECT m_screenOrigin{}; // virtual screen top-left stored as left/top
    UINT m_dpi = 96;
    SelectionModel m_model;

    // D2D / DWrite / GDI layered staging
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
};

} // namespace snip
