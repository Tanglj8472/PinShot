//==============================================================================
// SnipClone — fullscreen selection overlay on a frozen capture
//
// PinShot: capture virtual desktop first, paint that pixmap, dim outside the
// selection (selection shows the freeze undimmed — not a live desktop hole).
//
// ZoomIt SelectRectangle: drag / ESC / topmost layered chrome (MIT patterns).
//==============================================================================
#include "SelectionOverlay.h"

#include "../Common/DpiHelper.h"
#include "../Common/WinHelper.h"

#include <windowsx.h>

#include <cwchar>
#include <vector>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")

namespace snip
{

SelectionModel SelectionOverlay::s_lastSelection{};
CaptureResult SelectionOverlay::s_lastCrop{};

namespace
{
constexpr wchar_t kClassName[] = L"SnipCloneSelectionOverlay";
constexpr LONG kMinSelectionPx = 2;

ID2D1Bitmap* CreatePremulBitmap(ID2D1RenderTarget* rt, const CaptureResult& image)
{
    if (!rt || image.Empty())
    {
        return nullptr;
    }

    std::vector<std::uint8_t> premul(image.bgra.size());
    for (int y = 0; y < image.height; ++y)
    {
        const auto* src = image.bgra.data() + static_cast<size_t>(y) * image.stride;
        auto* dst = premul.data() + static_cast<size_t>(y) * image.stride;
        for (int x = 0; x < image.width; ++x)
        {
            const std::uint8_t b = src[x * 4 + 0];
            const std::uint8_t g = src[x * 4 + 1];
            const std::uint8_t r = src[x * 4 + 2];
            const std::uint8_t a = src[x * 4 + 3] ? src[x * 4 + 3] : 255;
            dst[x * 4 + 0] = static_cast<std::uint8_t>(b * a / 255);
            dst[x * 4 + 1] = static_cast<std::uint8_t>(g * a / 255);
            dst[x * 4 + 2] = static_cast<std::uint8_t>(r * a / 255);
            dst[x * 4 + 3] = a;
        }
    }

    D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
    ID2D1Bitmap* bmp = nullptr;
    rt->CreateBitmap(
        D2D1::SizeU(static_cast<UINT32>(image.width), static_cast<UINT32>(image.height)),
        premul.data(),
        static_cast<UINT32>(image.stride),
        props,
        &bmp);
    return bmp;
}
} // namespace

SelectionOverlay::SelectionOverlay() = default;

SelectionOverlay::~SelectionOverlay()
{
    if (m_hwnd)
    {
        DestroyWindow(m_hwnd);
        m_hwnd = nullptr;
    }
    ReleaseAllResources();
}

const SelectionModel& SelectionOverlay::LastSelection()
{
    return s_lastSelection;
}

const CaptureResult& SelectionOverlay::LastCrop()
{
    return s_lastCrop;
}

bool SelectionOverlay::Start(
    HINSTANCE instance,
    HWND owner,
    CaptureResult freezeFrame,
    DoneCallback callback)
{
    if (m_hwnd || freezeFrame.Empty())
    {
        return false;
    }

    m_instance = instance;
    m_owner = owner;
    m_callback = std::move(callback);
    m_dragging = false;
    m_model.Clear();
    m_freeze = std::move(freezeFrame);

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = StaticWndProc;
    wc.hInstance = instance;
    wc.hCursor = LoadCursorW(nullptr, IDC_CROSS);
    wc.lpszClassName = kClassName;
    if (!EnsureWindowClass(wc))
    {
        return false;
    }

    m_hwnd = CreateTopmostLayeredPopup(owner, instance, kClassName, this);
    if (!m_hwnd)
    {
        return false;
    }

    SetWindowDisplayAffinity(m_hwnd, WDA_EXCLUDEFROMCAPTURE);

    ShowWindow(m_hwnd, SW_SHOW);
    SetForegroundWindow(m_hwnd);
    SetFocus(m_hwnd);
    Present();
    return true;
}

void SelectionOverlay::Stop(bool confirmed)
{
    if (!m_hwnd)
    {
        return;
    }

    if (confirmed && m_model.hasSelection)
    {
        s_lastSelection = m_model;
        s_lastCrop = m_freeze.CropToScreenRect(m_model.screenRect);
    }
    else
    {
        m_model.Clear();
        s_lastCrop = {};
    }

    const HWND hwnd = m_hwnd;
    m_hwnd = nullptr;

    if (GetCapture() == hwnd)
    {
        ReleaseCapture();
    }

    DestroyWindow(hwnd);

    if (m_callback)
    {
        m_callback(confirmed && s_lastSelection.hasSelection, s_lastSelection);
    }

    if (m_owner && IsWindow(m_owner))
    {
        PostMessageW(
            m_owner,
            WM_SNIP_SELECTION_DONE,
            confirmed && s_lastSelection.hasSelection && !s_lastCrop.Empty() ? 1 : 0,
            0);
    }
}

LRESULT CALLBACK SelectionOverlay::StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        auto* self = static_cast<SelectionOverlay*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->m_hwnd = hwnd;
        return TRUE;
    }
    auto* self = reinterpret_cast<SelectionOverlay*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if (!self)
    {
        return DefWindowProcW(hwnd, msg, wParam, lParam);
    }
    return self->WndProc(hwnd, msg, wParam, lParam);
}

LRESULT SelectionOverlay::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_CREATE:
    {
        m_dpi = GetDpiForWindowHelper(hwnd);
        GetClientRect(hwnd, &m_clientBounds);
        const RECT vs = GetVirtualScreenRect();
        m_screenOrigin.left = vs.left;
        m_screenOrigin.top = vs.top;
        return 0;
    }

    case WM_LBUTTONDOWN:
    {
        SetCapture(hwnd);
        m_dragging = true;
        m_ptStart = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        m_ptCurrent = m_ptStart;
        Present();
        return 0;
    }

    case WM_MOUSEMOVE:
        if (m_dragging && GetCapture() == hwnd)
        {
            m_ptCurrent = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
            RECT local = RectFromPointsMinSize(m_ptStart, m_ptCurrent, kMinSelectionPx);
            local = ForceRectInBounds(local, m_clientBounds);

            RECT screen = local;
            OffsetRect(&screen, m_screenOrigin.left, m_screenOrigin.top);
            m_model.Set(screen);
            Present();
        }
        return 0;

    case WM_LBUTTONUP:
    {
        if (!m_dragging)
        {
            return 0;
        }
        m_dragging = false;
        if (GetCapture() == hwnd)
        {
            ReleaseCapture();
        }

        m_ptCurrent = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        RECT local = RectFromPointsMinSize(m_ptStart, m_ptCurrent, kMinSelectionPx);
        local = ForceRectInBounds(local, m_clientBounds);
        RECT screen = local;
        OffsetRect(&screen, m_screenOrigin.left, m_screenOrigin.top);
        m_model.Set(screen);

        Stop(m_model.hasSelection);
        return 0;
    }

    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE)
        {
            Stop(false);
            return 0;
        }
        break;

    case WM_RBUTTONUP:
        Stop(false);
        return 0;

    case WM_DISPLAYCHANGE:
    {
        const RECT vs = GetVirtualScreenRect();
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            vs.left,
            vs.top,
            vs.right - vs.left,
            vs.bottom - vs.top,
            SWP_NOACTIVATE);
        GetClientRect(hwnd, &m_clientBounds);
        m_screenOrigin.left = vs.left;
        m_screenOrigin.top = vs.top;
        ReleaseSurfaceResources();
        Present();
        return 0;
    }

    case WM_DPICHANGED:
        m_dpi = HIWORD(wParam);
        Present();
        return 0;

    case WM_DESTROY:
        ReleaseAllResources();
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return 0;

    default:
        break;
    }

    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

bool SelectionOverlay::EnsureFreezeBitmap()
{
    if (m_freezeBmp)
    {
        return true;
    }
    if (!m_rt || m_freeze.Empty())
    {
        return false;
    }
    m_freezeBmp = CreatePremulBitmap(m_rt, m_freeze);
    return m_freezeBmp != nullptr;
}

bool SelectionOverlay::EnsureRenderResources(int width, int height)
{
    if (width <= 0 || height <= 0)
    {
        return false;
    }

    if (m_bitmapW == width && m_bitmapH == height && m_rt && m_dib && m_d2dFactory && m_freezeBmp)
    {
        return true;
    }

    ReleaseSurfaceResources();

    if (!m_d2dFactory)
    {
        if (FAILED(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &m_d2dFactory)))
        {
            return false;
        }
    }
    if (!m_dwrite)
    {
        if (FAILED(DWriteCreateFactory(
                DWRITE_FACTORY_TYPE_SHARED,
                __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown**>(&m_dwrite))))
        {
            return false;
        }
    }
    if (!m_textFormat)
    {
        if (FAILED(m_dwrite->CreateTextFormat(
                L"Segoe UI",
                nullptr,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                14.0f,
                L"en-us",
                &m_textFormat)))
        {
            return false;
        }
        m_textFormat->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        m_textFormat->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
    }

    m_screenDc = GetDC(nullptr);
    m_memDc = CreateCompatibleDC(m_screenDc);

    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height; // top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    m_dib = CreateDIBSection(m_memDc, &bmi, DIB_RGB_COLORS, &m_bits, nullptr, 0);
    if (!m_dib || !m_bits)
    {
        ReleaseSurfaceResources();
        return false;
    }
    m_oldBitmap = SelectObject(m_memDc, m_dib);

    D2D1_RENDER_TARGET_PROPERTIES props = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
        static_cast<FLOAT>(kDpiBaseline),
        static_cast<FLOAT>(kDpiBaseline),
        D2D1_RENDER_TARGET_USAGE_NONE,
        D2D1_FEATURE_LEVEL_DEFAULT);

    if (FAILED(m_d2dFactory->CreateDCRenderTarget(&props, &m_rt)))
    {
        ReleaseSurfaceResources();
        return false;
    }

    RECT bind{ 0, 0, width, height };
    if (FAILED(m_rt->BindDC(m_memDc, &bind)))
    {
        ReleaseSurfaceResources();
        return false;
    }

    m_rt->CreateSolidColorBrush(D2D1::ColorF(1.f, 1.f, 1.f, 1.f), &m_brushWhite);
    m_rt->CreateSolidColorBrush(D2D1::ColorF(0.f, 0.f, 0.f, 0.45f), &m_brushDim);

    m_bitmapW = width;
    m_bitmapH = height;

    if (!EnsureFreezeBitmap())
    {
        ReleaseSurfaceResources();
        return false;
    }
    return m_brushWhite && m_brushDim;
}

void SelectionOverlay::ReleaseSurfaceResources()
{
    if (m_freezeBmp)
    {
        m_freezeBmp->Release();
        m_freezeBmp = nullptr;
    }
    if (m_brushWhite)
    {
        m_brushWhite->Release();
        m_brushWhite = nullptr;
    }
    if (m_brushDim)
    {
        m_brushDim->Release();
        m_brushDim = nullptr;
    }
    if (m_rt)
    {
        m_rt->Release();
        m_rt = nullptr;
    }

    if (m_memDc)
    {
        if (m_oldBitmap)
        {
            SelectObject(m_memDc, m_oldBitmap);
            m_oldBitmap = nullptr;
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
    m_bitmapW = 0;
    m_bitmapH = 0;
}

void SelectionOverlay::ReleaseAllResources()
{
    ReleaseSurfaceResources();

    if (m_textFormat)
    {
        m_textFormat->Release();
        m_textFormat = nullptr;
    }
    if (m_dwrite)
    {
        m_dwrite->Release();
        m_dwrite = nullptr;
    }
    if (m_d2dFactory)
    {
        m_d2dFactory->Release();
        m_d2dFactory = nullptr;
    }
    m_freeze = {};
}

void SelectionOverlay::RenderToBitmap()
{
    if (!m_rt || !m_freezeBmp)
    {
        return;
    }

    const float w = static_cast<float>(m_bitmapW);
    const float h = static_cast<float>(m_bitmapH);

    m_rt->BeginDraw();
    m_rt->Clear(D2D1::ColorF(0, 0, 0, 0));

    // Frozen desktop (PinShot paintEvent: drawPixmap(shot)).
    m_rt->DrawBitmap(m_freezeBmp, D2D1::RectF(0, 0, w, h));

    if (m_model.hasSelection)
    {
        const float l = static_cast<float>(m_model.screenRect.left - m_screenOrigin.left);
        const float t = static_cast<float>(m_model.screenRect.top - m_screenOrigin.top);
        const float r = static_cast<float>(m_model.screenRect.right - m_screenOrigin.left);
        const float b = static_cast<float>(m_model.screenRect.bottom - m_screenOrigin.top);

        // Dim outside selection — keep freeze visible inside (no transparent hole).
        if (t > 0.f)
        {
            m_rt->FillRectangle(D2D1::RectF(0, 0, w, t), m_brushDim);
        }
        if (b < h)
        {
            m_rt->FillRectangle(D2D1::RectF(0, b, w, h), m_brushDim);
        }
        if (l > 0.f && b > t)
        {
            m_rt->FillRectangle(D2D1::RectF(0, t, l, b), m_brushDim);
        }
        if (r < w && b > t)
        {
            m_rt->FillRectangle(D2D1::RectF(r, t, w, b), m_brushDim);
        }

        const float stroke = static_cast<float>(ScaleForDpi(2, m_dpi));
        m_rt->DrawRectangle(D2D1::RectF(l, t, r, b), m_brushWhite, stroke);

        const float tick = static_cast<float>(ScaleForDpi(8, m_dpi));
        m_rt->DrawLine(D2D1::Point2F(l, t), D2D1::Point2F(l + tick, t), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(l, t), D2D1::Point2F(l, t + tick), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(r, t), D2D1::Point2F(r - tick, t), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(r, t), D2D1::Point2F(r, t + tick), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(l, b), D2D1::Point2F(l + tick, b), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(l, b), D2D1::Point2F(l, b - tick), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(r, b), D2D1::Point2F(r - tick, b), m_brushWhite, stroke);
        m_rt->DrawLine(D2D1::Point2F(r, b), D2D1::Point2F(r, b - tick), m_brushWhite, stroke);

        if (m_textFormat)
        {
            wchar_t label[64]{};
            swprintf_s(label, L"%ld × %ld", m_model.Width(), m_model.Height());

            ID2D1SolidColorBrush* bg = nullptr;
            ID2D1SolidColorBrush* fg = nullptr;
            m_rt->CreateSolidColorBrush(D2D1::ColorF(0.f, 0.f, 0.f, 0.7f), &bg);
            m_rt->CreateSolidColorBrush(D2D1::ColorF(1.f, 1.f, 1.f, 1.f), &fg);

            const float labelW = 120.f;
            const float labelH = 22.f;
            float lx = l;
            float ly = t - labelH - 6.f;
            if (ly < 4.f)
            {
                ly = t + 6.f;
            }
            if (lx + labelW > w)
            {
                lx = w - labelW - 4.f;
            }

            if (bg && fg)
            {
                m_rt->FillRectangle(D2D1::RectF(lx, ly, lx + labelW, ly + labelH), bg);
                m_rt->DrawText(
                    label,
                    static_cast<UINT32>(wcslen(label)),
                    m_textFormat,
                    D2D1::RectF(lx + 8.f, ly + 2.f, lx + labelW, ly + labelH),
                    fg);
            }
            if (bg)
            {
                bg->Release();
            }
            if (fg)
            {
                fg->Release();
            }
        }
    }
    else
    {
        // No selection yet: full dim over freeze (PinShot idle).
        m_rt->FillRectangle(D2D1::RectF(0, 0, w, h), m_brushDim);
    }

    HRESULT hr = m_rt->EndDraw();
    if (hr == D2DERR_RECREATE_TARGET)
    {
        ReleaseSurfaceResources();
    }
}

void SelectionOverlay::Present()
{
    if (!m_hwnd)
    {
        return;
    }

    RECT rc{};
    GetClientRect(m_hwnd, &rc);
    const int width = rc.right - rc.left;
    const int height = rc.bottom - rc.top;
    if (!EnsureRenderResources(width, height))
    {
        return;
    }

    RECT bind{ 0, 0, width, height };
    if (FAILED(m_rt->BindDC(m_memDc, &bind)))
    {
        return;
    }

    RenderToBitmap();

    POINT ptSrc{ 0, 0 };
    POINT ptDst{};
    SIZE size{ width, height };
    RECT wnd{};
    GetWindowRect(m_hwnd, &wnd);
    ptDst.x = wnd.left;
    ptDst.y = wnd.top;

    BLENDFUNCTION blend{};
    blend.BlendOp = AC_SRC_OVER;
    blend.SourceConstantAlpha = 255;
    blend.AlphaFormat = AC_SRC_ALPHA;

    UpdateLayeredWindow(
        m_hwnd,
        m_screenDc,
        &ptDst,
        &size,
        m_memDc,
        &ptSrc,
        0,
        &blend,
        ULW_ALPHA);
}

} // namespace snip
