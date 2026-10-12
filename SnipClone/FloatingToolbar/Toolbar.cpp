//==============================================================================
// SnipClone — floating toolbar
//==============================================================================
#include "Toolbar.h"

#include "../Common/DpiHelper.h"
#include "../Common/WinHelper.h"

#include <windowsx.h>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipCloneToolbar";
constexpr int kBtn = 36;
constexpr int kPad = 6;
constexpr int kGap = 4;
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

    BuildButtons();
    LayoutButtons();

    const int width = m_buttons.empty() ? 100 : m_buttons.back().hit.right + kPad;
    const int height = kPad * 2 + kBtn;

    m_hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
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
    Present();
    return true;
}

void Toolbar::Destroy()
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

void Toolbar::BuildButtons()
{
    m_buttons.clear();
    auto addTool = [&](ToolKind k, const wchar_t* id, const wchar_t* tip) {
        ToolButton b;
        b.action = ToolbarAction::SetTool;
        b.tool = k;
        b.id = id;
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

    // Order mirrors PinShot snip dock (subset): tools | undo redo | close pin save copy
    addTool(ToolKind::Pen, L"pen", L"Pen");
    addTool(ToolKind::Rect, L"rect", L"Rectangle");
    addTool(ToolKind::Arrow, L"arrow", L"Arrow");
    addAct(ToolbarAction::Undo, L"undo", L"Undo");
    addAct(ToolbarAction::Redo, L"redo", L"Redo");
    addAct(ToolbarAction::Close, L"close", L"Cancel");
    addAct(ToolbarAction::Pin, L"pin", L"Pin");
    addAct(ToolbarAction::Save, L"save", L"Save");
    addAct(ToolbarAction::Copy, L"copy", L"Copy");
}

void Toolbar::LayoutButtons()
{
    int x = kPad;
    for (auto& b : m_buttons)
    {
        // Visual separator gap before undo and before close (PinShot separators).
        if (b.action == ToolbarAction::Undo || b.action == ToolbarAction::Close)
        {
            x += 8;
        }
        b.hit = { x, kPad, x + kBtn, kPad + kBtn };
        x += kBtn + kGap;
    }
}

void Toolbar::PlaceBelow(const RECT& annotationScreenRect)
{
    if (!m_hwnd)
    {
        return;
    }
    RECT rc{};
    GetClientRect(m_hwnd, &rc);
    const int tw = rc.right - rc.left;
    const int th = rc.bottom - rc.top;

    int x = annotationScreenRect.right - tw;
    int y = annotationScreenRect.bottom + 8;
    if (x < annotationScreenRect.left)
    {
        x = annotationScreenRect.left;
    }

    // Flip above if near virtual screen bottom (PinShot _place_dock).
    const RECT vs = GetVirtualScreenRect();
    if (y + th > vs.bottom)
    {
        y = annotationScreenRect.top - th - 8;
    }

    SetWindowPos(m_hwnd, HWND_TOPMOST, x, y, tw, th, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    Present();
}

void Toolbar::SyncFromAnnotator()
{
    if (!m_ann)
    {
        return;
    }
    for (auto& b : m_buttons)
    {
        b.checked = (b.action == ToolbarAction::SetTool && b.tool == m_ann->Tool());
    }
    Present();
}

void Toolbar::Show(bool visible)
{
    if (m_hwnd)
    {
        ShowWindow(m_hwnd, visible ? SW_SHOWNA : SW_HIDE);
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
            if (b->action == ToolbarAction::SetTool && m_ann)
            {
                // Toggle like PinShot: click active tool to clear.
                m_ann->SetTool(m_ann->Tool() == b->tool ? ToolKind::None : b->tool);
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
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
    if (FAILED(m_factory->CreateDCRenderTarget(&props, &m_rt)))
    {
        return false;
    }
    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);
    m_rt->CreateSolidColorBrush(D2D1::ColorF(1, 1, 1), &m_brush);
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
    m_rt->Clear(D2D1::ColorF(0.12f, 0.12f, 0.14f, 0.92f));

    for (const auto& b : m_buttons)
    {
        D2D1_RECT_F r = D2D1::RectF(
            static_cast<float>(b.hit.left),
            static_cast<float>(b.hit.top),
            static_cast<float>(b.hit.right),
            static_cast<float>(b.hit.bottom));

        if (b.checked)
        {
            m_brush->SetColor(D2D1::ColorF(0.20f, 0.45f, 0.85f, 1.f));
            m_rt->FillRectangle(r, m_brush);
        }
        else if (b.hover)
        {
            m_brush->SetColor(D2D1::ColorF(1.f, 1.f, 1.f, 0.18f));
            m_rt->FillRectangle(r, m_brush);
        }

        m_brush->SetColor(D2D1::ColorF(1.f, 1.f, 1.f, 1.f));
        if (m_text)
        {
            m_rt->DrawText(
                b.id.c_str(),
                static_cast<UINT32>(b.id.size()),
                m_text,
                r,
                m_brush);
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
