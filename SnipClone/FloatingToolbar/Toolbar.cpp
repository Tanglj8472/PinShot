//==============================================================================
// SnipClone — floating toolbar (PinShot ToolDock main bar)
//==============================================================================
#include "Toolbar.h"

#include "ToolIcons.h"
#include "../Common/DpiHelper.h"
#include "../Common/WinHelper.h"

#include <commctrl.h>
#include <windowsx.h>

#include <string>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")
#pragma comment(lib, "comctl32.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipCloneToolbar";
constexpr int kBtnDip = 34;
constexpr int kPadDip = 5;
constexpr int kGapDip = 2;
constexpr int kSepDip = 8;

constexpr D2D1_COLOR_F kBg = { 0.97f, 0.97f, 0.97f, 0.96f };
constexpr D2D1_COLOR_F kBorder = { 0.60f, 0.60f, 0.60f, 1.f };
constexpr D2D1_COLOR_F kHover = { 0.87f, 0.91f, 0.96f, 1.f };
constexpr D2D1_COLOR_F kChecked = { 0.77f, 0.85f, 0.95f, 1.f };
constexpr D2D1_COLOR_F kFg = { 0.20f, 0.20f, 0.20f, 1.f };
constexpr D2D1_COLOR_F kDisabled = { 0.70f, 0.70f, 0.70f, 1.f };
constexpr D2D1_COLOR_F kSepColor = { 0.78f, 0.78f, 0.78f, 1.f };
}

int Toolbar::Dip(int value) const
{
    return ScaleForDpi(value, m_dpi);
}

Toolbar::~Toolbar()
{
    Destroy();
}

bool Toolbar::Create(HINSTANCE instance, HWND owner, DrawStack* annotator, ActionFn onAction)
{
    if (m_hwnd)
    {
        return false;
    }
    m_instance = instance;
    m_owner = owner;
    m_ann = annotator;
    m_onAction = std::move(onAction);

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = StaticWndProc;
    wc.hInstance = instance;
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.lpszClassName = kClass;
    EnsureWindowClass(wc);

    m_dpi = 96;
    m_btn = kBtnDip;
    m_pad = kPadDip;
    m_gap = kGapDip;
    m_sep = kSepDip;

    BuildButtons();
    LayoutButtons();

    const int width = m_buttons.empty() ? 100 : m_buttons.back().hit.right + m_pad;
    const int height = m_pad * 2 + m_btn;

    m_hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        kClass,
        L"",
        WS_POPUP,
        0,
        0,
        width,
        height,
        owner,
        nullptr,
        instance,
        this);
    if (!m_hwnd)
    {
        return false;
    }

    SetWindowDisplayAffinity(m_hwnd, WDA_EXCLUDEFROMCAPTURE);
    EnsureTooltip();

    m_stylePanel = std::make_unique<StylePanel>();
    m_stylePanel->Create(instance, owner, annotator, m_onAction);

    Present();
    SyncStylePanel();
    return true;
}

void Toolbar::EnsureTooltip()
{
    if (m_tooltip || !m_hwnd)
    {
        return;
    }
    INITCOMMONCONTROLSEX icc{ sizeof(icc), ICC_BAR_CLASSES };
    InitCommonControlsEx(&icc);
    m_tooltip = CreateWindowExW(
        WS_EX_TOPMOST,
        TOOLTIPS_CLASSW,
        nullptr,
        WS_POPUP | TTS_NOPREFIX | TTS_ALWAYSTIP,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        m_hwnd,
        nullptr,
        m_instance,
        nullptr);
    if (!m_tooltip)
    {
        return;
    }
    SetWindowPos(m_tooltip, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    TOOLINFOW ti{};
    ti.cbSize = sizeof(ti);
    ti.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    ti.hwnd = m_hwnd;
    ti.uId = reinterpret_cast<UINT_PTR>(m_hwnd);
    ti.lpszText = const_cast<wchar_t*>(L"");
    SendMessageW(m_tooltip, TTM_ADDTOOLW, 0, reinterpret_cast<LPARAM>(&ti));
    SendMessageW(m_tooltip, TTM_SETMAXTIPWIDTH, 0, 320);
}

void Toolbar::UpdateTooltipFor(const ToolButton* btn)
{
    if (!m_tooltip || !m_hwnd)
    {
        return;
    }
    TOOLINFOW ti{};
    ti.cbSize = sizeof(ti);
    ti.hwnd = m_hwnd;
    ti.uId = reinterpret_cast<UINT_PTR>(m_hwnd);
    ti.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    ti.lpszText = btn ? const_cast<wchar_t*>(btn->tip.c_str()) : const_cast<wchar_t*>(L"");
    SendMessageW(m_tooltip, TTM_UPDATETIPTEXTW, 0, reinterpret_cast<LPARAM>(&ti));
}

void Toolbar::Destroy()
{
    if (m_tooltip)
    {
        DestroyWindow(m_tooltip);
        m_tooltip = nullptr;
    }
    if (m_stylePanel)
    {
        m_stylePanel->Destroy();
        m_stylePanel.reset();
    }
    if (m_hwnd)
    {
        DestroyWindow(m_hwnd);
        m_hwnd = nullptr;
    }
    ReleaseSurface();
    if (m_text)
    {
        m_text->Release();
        m_text = nullptr;
    }
    if (m_dwrite)
    {
        m_dwrite->Release();
        m_dwrite = nullptr;
    }
    if (m_factory)
    {
        m_factory->Release();
        m_factory = nullptr;
    }
}

void Toolbar::BuildButtons()
{
    m_buttons.clear();
    auto addTool = [&](ToolKind k, const wchar_t* tip) {
        ToolButton b;
        b.action = ToolbarAction::SetTool;
        b.tool = k;
        b.id = ToolKindName(k);
        b.tip = tip;
        m_buttons.push_back(b);
    };
    auto addAct = [&](ToolbarAction a, const wchar_t* id, const wchar_t* tip) {
        ToolButton b;
        b.action = a;
        b.id = id;
        b.tip = tip;
        m_buttons.push_back(b);
    };

    // 参考工具-like order: select | shapes | arrow | pen | mosaic | text | eraser
    // | undo redo | cancel pin save copy. Highlighter deferred.
    addTool(ToolKind::Select, L"选择 (S) — 选中/移动/缩放图形");
    addTool(ToolKind::Rect, L"矩形 (R)  Shift=正方形");
    addTool(ToolKind::Ellipse, L"椭圆 (E)  Shift=正圆");
    addTool(ToolKind::Arrow, L"箭头 (A)  Shift=约束方向");
    addTool(ToolKind::Pen, L"画笔 (P)");
    addTool(ToolKind::Mosaic, L"马赛克 (M)");
    addTool(ToolKind::Text, L"文字 (X)");
    addTool(ToolKind::Eraser, L"橡皮 (K)");
    addAct(ToolbarAction::Undo, L"undo", L"撤销 (Ctrl+Z)");
    addAct(ToolbarAction::Redo, L"redo", L"重做 (Ctrl+Y)");
    addAct(ToolbarAction::Close, L"close", L"取消 (Esc)");
    addAct(ToolbarAction::Pin, L"pin", L"贴图 (Ctrl+T)");
    addAct(ToolbarAction::Save, L"save", L"保存 (Ctrl+S)");
    addAct(ToolbarAction::Copy, L"copy", L"复制 (Ctrl+C)");
}

void Toolbar::LayoutButtons()
{
    m_btn = Dip(kBtnDip);
    m_pad = Dip(kPadDip);
    m_gap = Dip(kGapDip);
    m_sep = Dip(kSepDip);
    int x = m_pad;
    for (auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::Undo || b.action == ToolbarAction::Close)
        {
            x += m_sep;
        }
        b.hit = { x, m_pad, x + m_btn, m_pad + m_btn };
        x += m_btn + m_gap;
    }
}

void Toolbar::SyncStylePanel()
{
    if (!m_stylePanel || !m_ann)
    {
        return;
    }
    const ToolKind t = m_ann->Tool();
    if (t != m_panelTool)
    {
        m_panelTool = t;
        m_stylePanel->RebuildForTool(t);
    }
    else
    {
        m_stylePanel->SyncFromAnnotator();
    }

    if (t == ToolKind::None)
    {
        m_stylePanel->Show(false);
    }
    else
    {
        m_stylePanel->PlaceUnder(m_hwnd);
    }
}

void Toolbar::PlaceBelow(const RECT& annotationScreenRect)
{
    if (!m_hwnd)
    {
        return;
    }

    // A08: size from target monitor DPI; clamp to that monitor's work area.
    m_dpi = GetDpiForRect(annotationScreenRect);
    LayoutButtons();
    const int tw = m_buttons.empty() ? Dip(100) : m_buttons.back().hit.right + m_pad;
    const int th = m_pad * 2 + m_btn;
    SetWindowPos(m_hwnd, nullptr, 0, 0, tw, th, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);

    const int gap = Dip(8);
    const int panelReserve = Dip(44);
    int x = annotationScreenRect.right - tw;
    int y = annotationScreenRect.bottom + gap;
    if (x < annotationScreenRect.left)
    {
        x = annotationScreenRect.left;
    }

    const RECT work = GetMonitorWorkAreaFromRect(annotationScreenRect);
    if (y + th + panelReserve > work.bottom)
    {
        y = annotationScreenRect.top - th - panelReserve;
        if (y < work.top)
        {
            y = annotationScreenRect.top - th - gap;
        }
    }
    if (x + tw > work.right)
    {
        x = work.right - tw;
    }
    if (x < work.left)
    {
        x = work.left;
    }
    if (y + th > work.bottom)
    {
        y = work.bottom - th;
    }
    if (y < work.top)
    {
        y = work.top;
    }

    SetWindowPos(m_hwnd, HWND_TOPMOST, x, y, tw, th, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    Present();
    SyncStylePanel();
}

void Toolbar::SyncFromAnnotator()
{
    if (!m_ann)
    {
        return;
    }
    for (auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::SetTool)
        {
            b.checked = (b.tool == m_ann->Tool());
            b.enabled = true;
        }
        else if (b.action == ToolbarAction::Undo)
        {
            b.enabled = m_ann->CanUndo();
            b.checked = false;
        }
        else if (b.action == ToolbarAction::Redo)
        {
            b.enabled = m_ann->CanRedo();
            b.checked = false;
        }
        else
        {
            b.checked = false;
            b.enabled = true;
        }
    }
    Present();
    SyncStylePanel();
}

void Toolbar::Show(bool visible)
{
    if (m_hwnd)
    {
        ShowWindow(m_hwnd, visible ? SW_SHOWNA : SW_HIDE);
    }
    if (m_stylePanel)
    {
        if (!visible)
        {
            m_stylePanel->Show(false);
        }
        else
        {
            SyncStylePanel();
        }
    }
}

LRESULT CALLBACK Toolbar::StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        auto* self = static_cast<Toolbar*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->m_hwnd = hwnd;
        return TRUE;
    }
    auto* self = reinterpret_cast<Toolbar*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    return self ? self->WndProc(hwnd, msg, wParam, lParam) : DefWindowProcW(hwnd, msg, wParam, lParam);
}

ToolButton* Toolbar::HitTest(POINT client)
{
    for (auto& b : m_buttons)
    {
        if (PtInRect(&b.hit, client))
        {
            return &b;
        }
    }
    return nullptr;
}

LRESULT Toolbar::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_MOUSEMOVE:
    {
        POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        bool changed = false;
        for (auto& b : m_buttons)
        {
            const bool h = PtInRect(&b.hit, pt) != FALSE;
            if (b.hover != h)
            {
                b.hover = h;
                changed = true;
            }
        }
        UpdateTooltipFor(HitTest(pt));
        TRACKMOUSEEVENT tme{ sizeof(tme), TME_LEAVE, hwnd, 0 };
        TrackMouseEvent(&tme);
        if (changed)
        {
            Present();
        }
        return 0;
    }
    case WM_MOUSELEAVE:
        for (auto& b : m_buttons)
        {
            b.hover = false;
        }
        UpdateTooltipFor(nullptr);
        Present();
        return 0;
    case WM_LBUTTONUP:
    {
        POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        if (auto* b = HitTest(pt))
        {
            if (!b->enabled)
            {
                return 0;
            }
            if (b->action == ToolbarAction::SetTool && m_ann)
            {
                // A06: toggling off a draw tool returns to Select (crop-move / object edit), not None.
                m_ann->SetTool(m_ann->Tool() == b->tool ? ToolKind::Select : b->tool);
                SyncFromAnnotator();
            }
            if (m_onAction)
            {
                m_onAction(b->action, b->tool);
            }
        }
        return 0;
    }
    case WM_DESTROY:
        ReleaseSurface();
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

bool Toolbar::EnsureSurface(int w, int h)
{
    if (w <= 0 || h <= 0)
    {
        return false;
    }
    if (m_w == w && m_h == h && m_rt)
    {
        return true;
    }
    ReleaseSurface();

    if (!m_factory)
    {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &m_factory);
    }
    if (!m_dwrite)
    {
        DWriteCreateFactory(
            DWRITE_FACTORY_TYPE_SHARED,
            __uuidof(IDWriteFactory),
            reinterpret_cast<IUnknown**>(&m_dwrite));
        if (m_dwrite)
        {
            m_dwrite->CreateTextFormat(
                L"Segoe UI",
                nullptr,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                11.0f,
                L"en-us",
                &m_text);
            if (m_text)
            {
                m_text->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                m_text->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            }
        }
    }

    m_screenDc = GetDC(nullptr);
    m_memDc = CreateCompatibleDC(m_screenDc);
    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    m_dib = CreateDIBSection(m_memDc, &bmi, DIB_RGB_COLORS, &m_bits, nullptr, 0);
    if (!m_dib)
    {
        return false;
    }
    m_oldBmp = SelectObject(m_memDc, m_dib);

    D2D1_RENDER_TARGET_PROPERTIES props = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
        96.f,
        96.f);
    if (FAILED(m_factory->CreateDCRenderTarget(&props, &m_rt)))
    {
        return false;
    }
    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);
    m_rt->CreateSolidColorBrush(kFg, &m_brush);
    m_w = w;
    m_h = h;
    return true;
}

void Toolbar::ReleaseSurface()
{
    if (m_brush)
    {
        m_brush->Release();
        m_brush = nullptr;
    }
    if (m_rt)
    {
        m_rt->Release();
        m_rt = nullptr;
    }
    if (m_memDc)
    {
        if (m_oldBmp)
        {
            SelectObject(m_memDc, m_oldBmp);
            m_oldBmp = nullptr;
        }
        DeleteDC(m_memDc);
        m_memDc = nullptr;
    }
    if (m_dib)
    {
        DeleteObject(m_dib);
        m_dib = nullptr;
        m_bits = nullptr;
    }
    if (m_screenDc)
    {
        ReleaseDC(nullptr, m_screenDc);
        m_screenDc = nullptr;
    }
    m_w = m_h = 0;
}

void Toolbar::Present()
{
    if (!m_hwnd)
    {
        return;
    }
    RECT rc{};
    GetClientRect(m_hwnd, &rc);
    const int w = rc.right - rc.left;
    const int h = rc.bottom - rc.top;
    if (!EnsureSurface(w, h))
    {
        return;
    }

    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);
    m_rt->BeginDraw();
    m_rt->Clear(kBg);
    m_brush->SetColor(kBorder);
    m_rt->DrawRectangle(D2D1::RectF(0.5f, 0.5f, w - 0.5f, h - 0.5f), m_brush, 1.f);

    // Separators before undo and close groups.
    for (const auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::Undo || b.action == ToolbarAction::Close)
        {
            const float sx = static_cast<float>(b.hit.left) - static_cast<float>(m_sep) * 0.5f;
            m_brush->SetColor(kSepColor);
            m_rt->DrawLine(
                D2D1::Point2F(sx, static_cast<float>(m_pad + 4)),
                D2D1::Point2F(sx, static_cast<float>(m_pad + m_btn - 4)),
                m_brush,
                1.f);
        }
    }

    for (const auto& b : m_buttons)
    {
        D2D1_RECT_F r = D2D1::RectF(
            static_cast<float>(b.hit.left),
            static_cast<float>(b.hit.top),
            static_cast<float>(b.hit.right),
            static_cast<float>(b.hit.bottom));

        if (b.enabled && b.checked)
        {
            m_brush->SetColor(kChecked);
            m_rt->FillRectangle(r, m_brush);
        }
        else if (b.enabled && b.hover)
        {
            m_brush->SetColor(kHover);
            m_rt->FillRectangle(r, m_brush);
        }

        m_brush->SetColor(b.enabled ? kFg : kDisabled);
        DrawToolbarIcon(m_rt, m_brush, r, b.action, b.tool);
    }

    m_rt->EndDraw();

    POINT ptSrc{ 0, 0 };
    SIZE size{ w, h };
    POINT ptDst{};
    RECT wnd{};
    GetWindowRect(m_hwnd, &wnd);
    ptDst.x = wnd.left;
    ptDst.y = wnd.top;
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
