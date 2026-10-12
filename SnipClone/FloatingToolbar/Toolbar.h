//==============================================================================
// SnipClone — floating toolbar（装配对齐 PinShot ToolDock）
//
// 布局：pen | rect | arrow | undo redo | close pin save copy
// 与 Annotator 对话：SetTool；动作经回调交给 AnnotationOverlay 宿主。
//==============================================================================
#pragma once

#include "ToolButton.h"
#include "../AnnotationOverlay/DrawStack.h"

#include <Windows.h>
#include <d2d1.h>
#include <dwrite.h>

#include <functional>
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

    HWND m_hwnd = nullptr;
    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    DrawStack* m_ann = nullptr;
    ActionFn m_onAction;
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
