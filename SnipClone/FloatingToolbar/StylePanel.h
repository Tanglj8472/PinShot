//==============================================================================
// SnipClone — per-tool style sub-panel（对齐 PinShot ToolDock.panel）
//==============================================================================
#pragma once

#include "ToolButton.h"
#include "../AnnotationOverlay/DrawStack.h"
#include "../Common/WinCompat.h"

#include <d2d1.h>
#include <dwrite.h>

#include <functional>
#include <vector>

namespace snip
{

class StylePanel
{
public:
    using ActionFn = std::function<void(ToolbarAction action, ToolKind tool)>;

    StylePanel() = default;
    ~StylePanel();

    bool Create(HINSTANCE instance, HWND owner, DrawStack* annotator, ActionFn onAction);
    void Destroy();
    void RebuildForTool(ToolKind tool);
    void SyncFromAnnotator();
    void PlaceUnder(HWND toolbarHwnd);
    void Show(bool visible);
    HWND Window() const { return m_hwnd; }
    bool Visible() const { return m_hwnd && IsWindowVisible(m_hwnd); }

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    void LayoutButtons();
    void ResizeToContent();
    void Present();
    bool EnsureSurface(int w, int h);
    void ReleaseSurface();
    ToolButton* HitTest(POINT client);
    void PickCustomColor();

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    DrawStack* m_ann = nullptr;
    ActionFn m_onAction;
    ToolKind m_tool = ToolKind::None;
    std::vector<ToolButton> m_buttons;

    ID2D1Factory* m_factory = nullptr;
    ID2D1DCRenderTarget* m_rt = nullptr;
    IDWriteFactory* m_dwrite = nullptr;
    IDWriteTextFormat* m_text = nullptr;
    ID2D1SolidColorBrush* m_brush = nullptr;

    HDC m_screenDc = nullptr;
    HDC m_memDc = nullptr;
    HBITMAP m_dib = nullptr;
    HGDIOBJ m_oldBmp = nullptr;
    void* m_bits = nullptr;
    int m_w = 0;
    int m_h = 0;
};

} // namespace snip
