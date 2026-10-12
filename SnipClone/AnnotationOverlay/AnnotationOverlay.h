//==============================================================================
// SnipClone — annotation host（装配对齐 PinShot SnipOverlay）
//
// 拥有：底图 CaptureResult、DrawStack(Annotator)、Toolbar、导出动作。
// 窗口尺寸 = 选区截图；定位到选区屏幕坐标（非 ZoomIt 全屏）。
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "../FloatingToolbar/Toolbar.h"
#include "AnnotationRenderer.h"
#include "DrawStack.h"

#include <Windows.h>

#include <functional>
#include <memory>

namespace snip
{

inline constexpr UINT WM_SNIP_ANNOTATION_DONE = WM_APP + 0x52;

class AnnotationOverlay
{
public:
    using DoneFn = std::function<void()>;

    AnnotationOverlay() = default;
    ~AnnotationOverlay();

    bool Start(HINSTANCE instance, HWND owner, CaptureResult image);
    void Stop();
    bool IsActive() const { return m_hwnd != nullptr; }

    // Last composed export (copy/pin/save); pin manager may consume.
    static const CaptureResult& LastExport();
    static POINT LastExportOrigin();

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    void OnToolbarAction(ToolbarAction action, ToolKind tool);
    CaptureResult ComposeExport();
    void DoCopy();
    void DoSave();
    void DoPin();
    void RequestClose(); // deferred — safe when called from Toolbar WndProc
    void Present();
    bool EnsureSurface();
    void ReleaseSurface();

    static constexpr UINT WM_SNIP_CLOSE_SELF = WM_APP + 0x60;

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;

    CaptureResult m_base;
    DrawStack m_ann;
    AnnotationRenderer m_renderer;
    std::unique_ptr<Toolbar> m_toolbar;
    ID2D1Bitmap* m_baseBmp = nullptr;

    ID2D1DCRenderTarget* m_rt = nullptr;
    HDC m_screenDc = nullptr;
    HDC m_memDc = nullptr;
    HBITMAP m_dib = nullptr;
    HGDIOBJ m_oldBmp = nullptr;
    void* m_bits = nullptr;
    int m_w = 0;
    int m_h = 0;

    static CaptureResult s_lastExport;
    static POINT s_lastOrigin;
};

} // namespace snip
