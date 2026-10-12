//==============================================================================
// SnipClone — pin window（Layered 置顶 + 拖拽 / 缩放 / 右键菜单）
//==============================================================================
#pragma once

#include "../AnnotationOverlay/AnnotatorDocument.h"
#include "../Capture/CaptureResult.h"

#include "../Common/WinCompat.h"
#include <d2d1.h>

namespace snip
{

struct PinSnapshot
{
    CaptureResult image; // composed display cache
    AnnotatorDocument document;
    POINT origin{};
    float scale = 1.f;
    BYTE opacity = 255;
};

class PinWindow
{
public:
    PinWindow() = default;
    ~PinWindow();

    bool Create(HINSTANCE instance, CaptureResult image, POINT screenOrigin);
    bool Create(HINSTANCE instance, CaptureResult image, POINT screenOrigin, AnnotatorDocument document);
    bool CreateFromSnapshot(HINSTANCE instance, PinSnapshot snap);
    void Destroy();
    HWND Window() const { return m_hwnd; }

    PinSnapshot Snapshot() const;
    const CaptureResult& Image() const { return m_image; }
    const AnnotatorDocument& Document() const { return m_document; }
    bool HasEditableDocument() const { return m_document.Editable(); }

    // M2: replace composed cache + document after re-edit commit.
    void ApplyEdit(CaptureResult composed, AnnotatorDocument document);
    void RequestEdit(); // notify main to reopen annotation overlay

    void SetScale(float scale);
    void SetOpacity(BYTE opacity);
    void ResetScale();
    void SetClickThrough(bool enabled);
    bool ClickThrough() const { return m_clickThrough; }
    void SetVisible(bool visible);
    bool IsVisible() const;

    void CopyImage();
    void SaveImage();

private:
    enum
    {
        IDM_EDIT = 4000,
        IDM_COPY = 4001,
        IDM_SAVE,
        IDM_RESET_SCALE,
        IDM_OPACITY_100,
        IDM_OPACITY_75,
        IDM_OPACITY_50,
        IDM_OPACITY_25,
        IDM_CLICK_THROUGH,
        IDM_HIDE_ALL,
        IDM_SHOW_ALL,
        IDM_RESTORE_LAST,
        IDM_CLOSE,
    };

    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    void Present();
    bool EnsureSurface();
    void ReleaseSurface();
    void ApplyWindowSize();
    void ShowContextMenu(POINT screen);
    int DisplayWidth() const;
    int DisplayHeight() const;

    HWND m_hwnd = nullptr;
    HINSTANCE m_instance = nullptr;
    CaptureResult m_image;
    AnnotatorDocument m_document;
    POINT m_origin{};
    float m_scale = 1.f;
    BYTE m_opacity = 255;
    bool m_clickThrough = false;

    bool m_dragging = false;
    POINT m_dragScreen{};
    POINT m_dragOrigin{};

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
