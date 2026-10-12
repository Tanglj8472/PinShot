//==============================================================================
// SnipClone — annotation host（装配对齐 PinShot SnipOverlay）
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "../FloatingToolbar/Toolbar.h"
#include "AnnotatorDocument.h"
#include "AnnotationRenderer.h"
#include "DrawStack.h"
#include "TextEditor.h"

#include "../Common/WinCompat.h"

#include <memory>
#include <vector>

namespace snip
{

class PinWindow;

inline constexpr UINT WM_SNIP_ANNOTATION_DONE = WM_APP + 0x52;

class AnnotationOverlay
{
public:
    AnnotationOverlay() = default;
    ~AnnotationOverlay();

    // crop = visible region; freeze = full virtual desktop (enables post-annotate refine).
    bool Start(HINSTANCE instance, HWND owner, CaptureResult crop, CaptureResult freeze = {});
    // M2: reopen from pin document; writeBack receives composed + document on commit.
    bool StartFromDocument(
        HINSTANCE instance,
        HWND owner,
        AnnotatorDocument document,
        PinWindow* writeBack);
    void Stop();
    bool IsActive() const { return m_hwnd != nullptr; }

    static const CaptureResult& LastExport();
    static POINT LastExportOrigin();

private:
    enum class CropHit
    {
        None,
        Body, // Space+drag (or edge handles) moves the crop rect
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
    void DoSaveProject(); // M3: .scdoc
    AnnotatorDocument BuildDocument() const;
    bool CommitWriteBack(); // updates target pin; returns false if compose failed
    void RequestClose();
    void RequestCloseDiscard(); // Esc cancel during pin re-edit
    void Present();
    void RequestPresent(); // A11: coalesce Present to one per message pump turn
    bool EnsureSurface();
    bool EnsureSurfaceSize(int w, int h);
    void ReleaseSurface();
    void StampCaptureToDib(const CaptureResult& image);
    void BlitCaptureToDibAt(const CaptureResult& image, int dstX, int dstY);

    CropHit HitCropHandle(POINT client) const;
    void ApplyCropDrag(POINT client);
    void ShowObjectContextMenu(POINT client);
    bool CommitCropRect(RECT newScreenRect);
    // Restore viewport from history without pushing undo (shapes already restored).
    bool ApplyCropFromHistory(RECT cropScreen);
    void SessionUndo();
    void SessionRedo();
    // M1: when LiveSettings().selectAfterDraw, switch to Select after a new shape commits.
    void MaybeSelectAfterCommit(ToolKind drawnKind);
    void PaintCropChrome(const RECT& hostScreen);
    void PaintSizeLabel(const RECT& hostScreen, const RECT& cropScreen);
    void DimOutsideCrop(const RECT& hostScreen, const RECT& cropScreen);
    void UpdateCropCursor(CropHit hit);
    bool IsSelectLikeTool() const;
    RECT CurrentHostScreen() const;

    static constexpr UINT WM_SNIP_CLOSE_SELF = WM_APP + 0x60;
    static constexpr UINT WM_SNIP_PRESENT = WM_APP + 0x61;

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;

    CaptureResult m_freeze; // full desktop; empty → crop refine disabled
    CaptureResult m_base;   // current crop shown in the host
    PinWindow* m_writeBackPin = nullptr;
    bool m_writeBackCommitted = false;
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
    RECT m_hostScreen{};        // layered window rect while presenting (may exceed crop)
    bool m_presentPending = false;

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
