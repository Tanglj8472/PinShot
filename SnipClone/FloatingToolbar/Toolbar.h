//==============================================================================
// SnipClone — floating toolbar（装配对齐 PinShot ToolDock）
//
// 主栏：工具图标 | undo redo | close pin save copy
// 样式：独立 StylePanel（线宽/填充/色板/文字样式）
//==============================================================================
#pragma once

#include "StylePanel.h"
#include "ToolButton.h"
#include "../AnnotationOverlay/DrawStack.h"
#include "../Common/WinCompat.h"

#include <d2d1.h>
#include <dwrite.h>

#include <functional>
#include <memory>
#include <vector>

namespace snip
{

class Toolbar
{
public:
    using ActionFn = std::function<void(ToolbarAction action, ToolKind tool)>;

    Toolbar() = default;
    ~Toolbar();

    bool Create(HINSTANCE instance, HWND owner, DrawStack* annotator, ActionFn onAction);
    void Destroy();
    void PlaceBelow(const RECT& annotationScreenRect);
    void SyncFromAnnotator();
    void Show(bool visible);
    HWND Window() const { return m_hwnd; }

private:
    static LRESULT CALLBACK StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    LRESULT WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);

    void BuildButtons();
    void LayoutButtons();
    void Present();
    bool EnsureSurface(int w, int h);
    void ReleaseSurface();
    ToolButton* HitTest(POINT client);
    void SyncStylePanel();
    void EnsureTooltip();
    void UpdateTooltipFor(const ToolButton* btn);
    int Dip(int value) const;

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HWND m_tooltip = nullptr;
    HINSTANCE m_instance = nullptr;
    DrawStack* m_ann = nullptr;
    ActionFn m_onAction;
    std::vector<ToolButton> m_buttons;
    std::unique_ptr<StylePanel> m_stylePanel;
    ToolKind m_panelTool = ToolKind::Count; // force rebuild sentinel
    UINT m_dpi = 96;
    int m_btn = 34;
    int m_pad = 5;
    int m_gap = 2;
    int m_sep = 8;

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
