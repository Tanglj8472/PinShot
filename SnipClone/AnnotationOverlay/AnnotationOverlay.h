//==============================================================================
// SnipClone — annotation host（装配对齐 PinShot SnipOverlay）
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "../FloatingToolbar/Toolbar.h"
#include "AnnotationRenderer.h"
#include "DrawStack.h"
#include "TextEditor.h"

#include "../Common/WinCompat.h"

#include <memory>

namespace snip
{

inline constexpr UINT WM_SNIP_ANNOTATION_DONE = WM_APP + 0x52;

class AnnotationOverlay
{
public:
    AnnotationOverlay() = default;
    ~AnnotationOverlay();

    // crop = visible region; freeze = full virtual desktop (enables post-annotate refine).
    bool Start(HINSTANCE instance, HWND owner, CaptureResult crop, CaptureResult freeze = {});
    void Stop();
    bool IsActive() const { return m_hwnd != nullptr; }

    static const CaptureResult& LastExport();
    static POINT LastExportOrigin();

private:
    enum class CropHit
    {
        None,
        Body, // unused — body is for shapes
        N,
        S,
        E,
        W,
        NE,
        NW,
        SE,
        SW,
    };

    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    void OnToolbarAction(ToolbarAction action, ToolKind tool);
    void OpenTextEditorAt(D2D1_POINT_2F imagePt, int editIndex);
    void OnTextEditorDone(bool committed, const std::wstring& text);
    CaptureResult ComposeExport();
    void DoCopy();
    void DoSave();
    void DoPin();
    void RequestClose();
    void Present();
    bool EnsureSurface();
    void ReleaseSurface();
    void StampCaptureToDib(const CaptureResult& image);

    CropHit HitCropHandle(POINT client) const;
    void ApplyCropDrag(POINT client);
    bool CommitCropRect(RECT newScreenRect);
    void PaintCropChrome();
    void UpdateCropCursor(CropHit hit);

    static constexpr UINT WM_SNIP_CLOSE_SELF = WM_APP + 0x60;

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;

    CaptureResult m_freeze; // full desktop; empty → crop refine disabled
    CaptureResult m_base;   // current crop shown in the host
    DrawStack m_ann;
    AnnotationRenderer m_renderer;
    std::unique_ptr<Toolbar> m_toolbar;
    std::unique_ptr<TextEditor> m_textEditor;
    int m_editingTextIndex = -1; // -1 = new text
    D2D1_POINT_2F m_textAnchor{};

    CropHit m_cropDrag = CropHit::None;
    POINT m_cropDragStart{};
    RECT m_cropDragStartScreen{};
    RECT m_cropPreviewScreen{}; // live rubber-band; committed on mouse-up

    // Layered host is GDI DIB + UpdateLayeredWindow. Scene pixels come from
    // AnnotationRenderer::Compose (WIC) — not a DC render target Clear/Draw path.
    HDC m_screenDc = nullptr;
    HDC m_memDc = nullptr;
    HBITMAP m_dib = nullptr;
    HGDIOBJ m_oldBmp = nullptr;
    void* m_bits = nullptr;
    int m_dibStride = 0; // bytes per row of CreateDIBSection (may exceed width*4)
    int m_w = 0;
    int m_h = 0;

    static CaptureResult s_lastExport;
    static POINT s_lastOrigin;
};

} // namespace snip
