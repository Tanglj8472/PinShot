//==============================================================================
// SnipClone — style sub-panel
//==============================================================================
#include "StylePanel.h"

#include "ToolIcons.h"
#include "../Common/DpiHelper.h"
#include "../Common/WinHelper.h"

#include <windowsx.h>

#include <algorithm>
#include <cmath>
#include <string>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")
#pragma comment(lib, "comdlg32.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipCloneStylePanel";

// PinShot-like light chrome
constexpr D2D1_COLOR_F kBg = { 0.97f, 0.97f, 0.97f, 0.96f };
constexpr D2D1_COLOR_F kBorder = { 0.60f, 0.60f, 0.60f, 1.f };
constexpr D2D1_COLOR_F kHover = { 0.87f, 0.91f, 0.96f, 1.f };
constexpr D2D1_COLOR_F kChecked = { 0.77f, 0.85f, 0.95f, 1.f };
constexpr D2D1_COLOR_F kFg = { 0.20f, 0.20f, 0.20f, 1.f };
}

int StylePanel::Dip(int value) const
{
    return ScaleForDpi(value, m_dpi);
}

StylePanel::~StylePanel()
{
    Destroy();
}

bool StylePanel::Create(HINSTANCE instance, HWND owner, DrawStack* annotator, ActionFn onAction)
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

    m_hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        kClass,
        L"",
        WS_POPUP,
        0,
        0,
        200,
        40,
        owner,
        nullptr,
        instance,
        this);
    if (!m_hwnd)
    {
        return false;
    }
    SetWindowDisplayAffinity(m_hwnd, WDA_EXCLUDEFROMCAPTURE);
    ShowWindow(m_hwnd, SW_HIDE);
    return true;
}

void StylePanel::Destroy()
{
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

namespace
{
const wchar_t* StyleKindLabel(ToolKind k)
{
    switch (k)
    {
    case ToolKind::Pen:
        return L"画笔";
    case ToolKind::Rect:
        return L"矩形";
    case ToolKind::Ellipse:
        return L"椭圆";
    case ToolKind::Arrow:
        return L"箭头";
    case ToolKind::Text:
        return L"文字";
    case ToolKind::Mosaic:
        return L"马赛克";
    default:
        return L"对象";
    }
}
} // namespace

void StylePanel::RebuildForTool(ToolKind tool)
{
    m_tool = tool;
    m_buttons.clear();

    auto addAct = [&](ToolbarAction a, const wchar_t* id, const wchar_t* tip) {
        ToolButton b;
        b.action = a;
        b.id = id;
        b.tip = tip;
        m_buttons.push_back(b);
    };

    auto addStyleControls = [&](ToolKind kind, bool selectedTarget) {
        ToolButton ctx;
        ctx.action = ToolbarAction::None;
        ctx.id = L"ctx";
        ctx.tip = selectedTarget
                      ? (std::wstring(StyleKindLabel(kind)) + L" · 已选中")
                      : (std::wstring(StyleKindLabel(kind)) + L" · 新建样式");
        m_buttons.push_back(ctx);

        if (kind == ToolKind::Rect || kind == ToolKind::Ellipse)
        {
            addAct(ToolbarAction::ToggleFill, L"fill", L"填充 / 描边 (F)");
        }
        if (kind == ToolKind::Text)
        {
            addAct(ToolbarAction::WidthDec, L"A-", L"字号减 (1)");
            addAct(ToolbarAction::WidthInc, L"A+", L"字号加 (2)");
            ToolButton val;
            val.action = ToolbarAction::None;
            val.id = L"fv";
            val.tip = L"字号";
            m_buttons.push_back(val);
            addAct(ToolbarAction::Bold, L"B", L"粗体");
            addAct(ToolbarAction::Italic, L"I", L"斜体");
            addAct(ToolbarAction::Outline, L"OL", L"描边");
        }
        else if (kind == ToolKind::Mosaic)
        {
            addAct(ToolbarAction::WidthDec, L"w-", L"颗粒减小 (1)");
            addAct(ToolbarAction::WidthInc, L"w+", L"颗粒增大 (2)");
            ToolButton val;
            val.action = ToolbarAction::None;
            val.id = L"mv";
            val.tip = L"颗粒大小";
            m_buttons.push_back(val);
        }
        else if (kind != ToolKind::None && kind != ToolKind::Select && kind != ToolKind::Eraser)
        {
            addAct(ToolbarAction::WidthDec, L"w-", L"线宽减 (1)");
            addAct(ToolbarAction::WidthInc, L"w+", L"线宽加 (2)");
            ToolButton val;
            val.action = ToolbarAction::None;
            val.id = L"wv";
            val.tip = L"线宽";
            m_buttons.push_back(val);
        }
        else if (kind == ToolKind::Eraser)
        {
            addAct(ToolbarAction::WidthDec, L"w-", L"橡皮减小 (1)");
            addAct(ToolbarAction::WidthInc, L"w+", L"橡皮增大 (2)");
            ToolButton val;
            val.action = ToolbarAction::None;
            val.id = L"wv";
            val.tip = L"橡皮大小";
            m_buttons.push_back(val);
        }

        if (ToolUsesColor(kind))
        {
            ToolButton cur;
            cur.action = ToolbarAction::ColorCycle;
            cur.id = L"cur";
            cur.tip = L"自定义颜色";
            cur.colorIndex = -2;
            m_buttons.push_back(cur);

            for (int i = 0; i < kPaletteCount; ++i)
            {
                ToolButton b;
                b.action = ToolbarAction::SetColor;
                b.colorIndex = i;
                b.id = L"c" + std::to_wstring(i);
                b.tip = L"颜色";
                m_buttons.push_back(b);
            }
        }
    };

    if (tool == ToolKind::None)
    {
        LayoutButtons();
        ResizeToContent();
        SyncFromAnnotator();
        return;
    }

    if (tool == ToolKind::Select)
    {
        const int sel = m_ann ? m_ann->SelectedIndex() : -1;
        if (sel >= 0 && m_ann && m_ann->ShapeAt(sel))
        {
            addStyleControls(m_ann->ShapeAt(sel)->kind, true);
        }
        else
        {
            ToolButton hint;
            hint.action = ToolbarAction::None;
            hint.id = L"hint";
            hint.tip = L"拖动调整 · Space拖选区 · 双击文字 · Del";
            m_buttons.push_back(hint);
        }
        LayoutButtons();
        ResizeToContent();
        SyncFromAnnotator();
        return;
    }

    addStyleControls(tool, m_ann && m_ann->SelectedIndex() >= 0 &&
                               m_ann->EditTargetKind() == tool);

    LayoutButtons();
    ResizeToContent();
    SyncFromAnnotator();
}

void StylePanel::LayoutButtons()
{
    m_btn = Dip(30);
    m_pad = Dip(6);
    m_gap = Dip(4);
    m_swatch = Dip(16);
    const int custom = Dip(26);
    const int gapColor = Dip(3);

    int x = m_pad;
    for (auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::None)
        {
            int bw = Dip(56);
            if (b.id == L"ctx" || b.id == L"hint")
            {
                bw = Dip(148);
            }
            else if (b.id == L"wv" || b.id == L"fv" || b.id == L"mv")
            {
                bw = Dip(56);
            }
            b.hit = { x, m_pad, x + bw, m_pad + m_btn };
            x = b.hit.right + m_gap;
            continue;
        }
        // Color chips: visual swatch ~16 DIP, hit target at least ~28 DIP (plan §3.2).
        int bw = m_btn;
        int bh = m_btn;
        if (b.action == ToolbarAction::SetColor)
        {
            bw = bh = (std::max)(m_swatch + Dip(12), Dip(28));
        }
        else if (b.colorIndex == -2)
        {
            bw = bh = custom;
        }
        const int y = m_pad + (m_btn - bh) / 2;
        b.hit = { x, y, x + bw, y + bh };
        x += bw + ((b.action == ToolbarAction::SetColor) ? gapColor : m_gap);
    }
}

void StylePanel::ResizeToContent()
{
    if (!m_hwnd)
    {
        return;
    }
    const int width = m_buttons.empty() ? Dip(120) : m_buttons.back().hit.right + m_pad;
    const int height = m_pad * 2 + m_btn;
    SetWindowPos(m_hwnd, nullptr, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    ReleaseSurface();
    Present();
}

void StylePanel::SyncFromAnnotator()
{
    if (!m_ann)
    {
        return;
    }
    const DrawStyle* target = m_ann->EditTargetStyle();
    const auto& textStyle = target && m_ann->EditTargetKind() == ToolKind::Text
                                ? *target
                                : m_ann->Style(ToolKind::Text);
    const bool filled = target && ToolUsesFill(m_ann->EditTargetKind())
                            ? target->filled
                            : m_ann->Style(ToolKind::Rect).filled;
    for (auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::SetColor)
        {
            b.checked = false;
            if (target && b.colorIndex >= 0)
            {
                const auto pal = DrawStack::PaletteColor(b.colorIndex);
                b.checked = std::abs(pal.r - target->color.r) < 0.02f &&
                            std::abs(pal.g - target->color.g) < 0.02f &&
                            std::abs(pal.b - target->color.b) < 0.02f;
            }
            else
            {
                b.checked = (b.colorIndex == m_ann->ColorIndex());
            }
        }
        else if (b.action == ToolbarAction::ToggleFill)
        {
            b.checked = filled;
        }
        else if (b.action == ToolbarAction::Bold)
        {
            b.checked = textStyle.bold;
        }
        else if (b.action == ToolbarAction::Italic)
        {
            b.checked = textStyle.italic;
        }
        else if (b.action == ToolbarAction::Outline)
        {
            b.checked = textStyle.outline;
        }
        else if (b.action == ToolbarAction::None && b.id == L"wv" && target)
        {
            b.tip = std::to_wstring(static_cast<int>(target->width + 0.5f)) + L"px";
        }
        else if (b.action == ToolbarAction::None && b.id == L"fv")
        {
            b.tip = std::to_wstring(static_cast<int>(textStyle.fontSize + 0.5f)) + L"pt";
        }
        else if (b.action == ToolbarAction::None && b.id == L"mv" && target)
        {
            b.tip = L"颗粒 " + std::to_wstring(target->mosaicSize);
        }
        else if (b.action == ToolbarAction::None && (b.id == L"ctx" || b.id == L"hint"))
        {
            // tip already set in RebuildForTool
        }
        else
        {
            b.checked = false;
        }
    }
    Present();
}

void StylePanel::PlaceUnder(HWND toolbarHwnd)
{
    if (!m_hwnd || !toolbarHwnd || m_buttons.empty())
    {
        Show(false);
        return;
    }

    RECT tr{};
    GetWindowRect(toolbarHwnd, &tr);
    // M2: match toolbar monitor DPI (was fixed 30/6/16 px).
    const UINT dpi = GetDpiForRect(tr);
    if (dpi != m_dpi)
    {
        m_dpi = dpi;
        LayoutButtons();
        ResizeToContent();
    }

    RECT pr{};
    GetClientRect(m_hwnd, &pr);
    const int pw = pr.right - pr.left;
    const int ph = pr.bottom - pr.top;

    int x = tr.left;
    int y = tr.bottom + Dip(4);
    const RECT work = GetMonitorWorkAreaFromRect(tr);
    if (y + ph > work.bottom)
    {
        y = tr.top - ph - Dip(4);
    }
    if (x + pw > work.right)
    {
        x = work.right - pw;
    }
    if (x < work.left)
    {
        x = work.left;
    }
    if (y < work.top)
    {
        y = work.top;
    }
    if (y + ph > work.bottom)
    {
        y = work.bottom - ph;
    }

    SetWindowPos(m_hwnd, HWND_TOPMOST, x, y, pw, ph, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    Present();
}

void StylePanel::Show(bool visible)
{
    if (m_hwnd)
    {
        ShowWindow(m_hwnd, visible ? SW_SHOWNA : SW_HIDE);
    }
}

void StylePanel::PickCustomColor()
{
    if (!m_ann)
    {
        return;
    }
    static COLORREF cust[16]{};
    const DrawStyle* target = m_ann->EditTargetStyle();
    const DrawStyle& st = target ? *target
                                 : m_ann->Style(m_tool == ToolKind::None || m_tool == ToolKind::Select
                                                    ? ToolKind::Pen
                                                    : m_tool);
    CHOOSECOLORW cc{};
    cc.lStructSize = sizeof(cc);
    cc.hwndOwner = m_hwnd;
    cc.lpCustColors = cust;
    cc.rgbResult = RGB(
        static_cast<BYTE>(st.color.r * 255.f),
        static_cast<BYTE>(st.color.g * 255.f),
        static_cast<BYTE>(st.color.b * 255.f));
    cc.Flags = CC_FULLOPEN | CC_RGBINIT;
    if (ChooseColorW(&cc))
    {
        m_ann->SetColorRgb(
            GetRValue(cc.rgbResult) / 255.f,
            GetGValue(cc.rgbResult) / 255.f,
            GetBValue(cc.rgbResult) / 255.f);
        SyncFromAnnotator();
        if (m_onAction)
        {
            m_onAction(ToolbarAction::ColorCycle, m_tool);
        }
    }
}

LRESULT CALLBACK StylePanel::StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        auto* self = static_cast<StylePanel*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->m_hwnd = hwnd;
        return TRUE;
    }
    auto* self = reinterpret_cast<StylePanel*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    return self ? self->WndProc(hwnd, msg, wParam, lParam) : DefWindowProcW(hwnd, msg, wParam, lParam);
}

ToolButton* StylePanel::HitTest(POINT client)
{
    for (auto& b : m_buttons)
    {
        if (b.action == ToolbarAction::None)
        {
            continue;
        }
        if (PtInRect(&b.hit, client))
        {
            return &b;
        }
    }
    return nullptr;
}

LRESULT StylePanel::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
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
        Present();
        return 0;
    case WM_LBUTTONUP:
    {
        POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        if (auto* b = HitTest(pt))
        {
            if (b->colorIndex == -2)
            {
                PickCustomColor();
                return 0;
            }
            if (b->action == ToolbarAction::SetColor && m_ann)
            {
                m_ann->SetColorIndex(b->colorIndex);
                SyncFromAnnotator();
            }
            if (m_onAction)
            {
                m_onAction(b->action, m_tool);
            }
            SyncFromAnnotator();
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

bool StylePanel::EnsureSurface(int w, int h)
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
                11.f,
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

void StylePanel::ReleaseSurface()
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

void StylePanel::Present()
{
    if (!m_hwnd || !IsWindowVisible(m_hwnd))
    {
        // Still allow paint when about to show
    }
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

    for (const auto& b : m_buttons)
    {
        D2D1_RECT_F r = D2D1::RectF(
            static_cast<float>(b.hit.left),
            static_cast<float>(b.hit.top),
            static_cast<float>(b.hit.right),
            static_cast<float>(b.hit.bottom));

        if (b.action == ToolbarAction::None)
        {
            m_brush->SetColor(kFg);
            if (m_text)
            {
                const std::wstring& label = !b.tip.empty() ? b.tip : b.id;
                m_text->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                m_rt->DrawText(label.c_str(), static_cast<UINT32>(label.size()), m_text, r, m_brush);
            }
            continue;
        }

        if (b.action == ToolbarAction::SetColor && b.colorIndex >= 0)
        {
            const float inset =
                (std::max)(0.f, (r.right - r.left - static_cast<float>(m_swatch)) * 0.5f);
            const D2D1_RECT_F sw = D2D1::RectF(r.left + inset, r.top + inset, r.right - inset,
                                               r.bottom - inset);
            m_brush->SetColor(DrawStack::PaletteColor(b.colorIndex));
            m_rt->FillRectangle(sw, m_brush);
            m_brush->SetColor(kBorder);
            m_rt->DrawRectangle(sw, m_brush, b.checked ? 2.f : 1.f);
            continue;
        }

        if (b.colorIndex == -2 && m_ann)
        {
            const DrawStyle* target = m_ann->EditTargetStyle();
            D2D1_COLOR_F c = target
                                 ? target->color
                                 : m_ann->Style(ToolUsesColor(m_tool) ? m_tool : ToolKind::Pen).color;
            m_brush->SetColor(c);
            m_rt->FillRectangle(r, m_brush);
            m_brush->SetColor(kBorder);
            m_rt->DrawRectangle(r, m_brush, 1.f);
            continue;
        }

        if (b.checked)
        {
            m_brush->SetColor(kChecked);
            m_rt->FillRectangle(r, m_brush);
        }
        else if (b.hover)
        {
            m_brush->SetColor(kHover);
            m_rt->FillRectangle(r, m_brush);
        }

        m_brush->SetColor(kFg);
        if (b.action == ToolbarAction::Bold || b.action == ToolbarAction::Italic ||
            b.action == ToolbarAction::Outline || b.id == L"A-" || b.id == L"A+")
        {
            if (m_text)
            {
                m_rt->DrawText(b.id.c_str(), static_cast<UINT32>(b.id.size()), m_text, r, m_brush);
            }
        }
        else
        {
            DrawToolbarIcon(m_rt, m_brush, r, b.action, ToolKind::None);
            m_brush->SetColor(kFg); // restore after Close red etc.
        }
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
