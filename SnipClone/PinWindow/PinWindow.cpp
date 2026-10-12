//==============================================================================
// SnipClone — pin window
//==============================================================================
#include "PinWindow.h"

#include "PinManager.h"
#include "../Common/WinHelper.h"

#include <vector>

#pragma comment(lib, "d2d1.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipClonePinWindow";
}

PinWindow::~PinWindow()
{
    // HWND already destroyed on the WM_NCDESTROY path; only release leftover GDI/D2D.
    ReleaseSurface();
    if (m_factory)
    {
        m_factory->Release();
        m_factory = nullptr;
    }
}

bool PinWindow::Create(HINSTANCE instance, CaptureResult image, POINT screenOrigin)
{
    if (m_hwnd || image.Empty())
    {
        return false;
    }
    m_image = std::move(image);
    m_origin = screenOrigin;

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = StaticWndProc;
    wc.hInstance = instance;
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.lpszClassName = kClass;
    EnsureWindowClass(wc);

    m_hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        kClass,
        L"Pin",
        WS_POPUP,
        screenOrigin.x,
        screenOrigin.y,
        m_image.width,
        m_image.height,
        nullptr,
        nullptr,
        instance,
        this);
    if (!m_hwnd)
    {
        return false;
    }

    ShowWindow(m_hwnd, SW_SHOWNA);
    Present();
    return true;
}

void PinWindow::Destroy()
{
    if (m_hwnd)
    {
        HWND hwnd = m_hwnd;
        m_hwnd = nullptr;
        DestroyWindow(hwnd);
    }
}

LRESULT CALLBACK PinWindow::StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        auto* self = static_cast<PinWindow*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->m_hwnd = hwnd;
        return TRUE;
    }
    auto* self = reinterpret_cast<PinWindow*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    return self ? self->WndProc(hwnd, msg, wParam, lParam) : DefWindowProcW(hwnd, msg, wParam, lParam);
}

LRESULT PinWindow::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_NCHITTEST:
        // Drag anywhere like a caption; don't steal keyboard focus aggressively.
        return HTCAPTION;
    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE || wParam == VK_DELETE)
        {
            PinManager::Instance().RequestClose(this);
            return 0;
        }
        break;
    case WM_NCLBUTTONDBLCLK:
        // WM_NCHITTEST returns HTCAPTION, so double-click arrives as non-client.
        PinManager::Instance().RequestClose(this);
        return 0;
    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;
    case WM_DESTROY:
        m_hwnd = nullptr;
        ReleaseSurface();
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return 0;
    case WM_NCDESTROY:
        // Safe self-delete after Win32 finishes tearing down the HWND.
        PinManager::Instance().OnNcDestroy(this);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

bool PinWindow::EnsureSurface()
{
    const int w = m_image.width;
    const int h = m_image.height;
    if (m_rt && m_w == w && m_h == h)
    {
        return true;
    }
    ReleaseSurface();
    if (!m_factory)
    {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &m_factory);
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
    m_factory->CreateDCRenderTarget(&props, &m_rt);
    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);

    // Premultiply + upload.
    std::vector<std::uint8_t> premul(m_image.bgra.size());
    for (size_t i = 0; i + 3 < m_image.bgra.size(); i += 4)
    {
        const std::uint8_t b = m_image.bgra[i + 0];
        const std::uint8_t g = m_image.bgra[i + 1];
        const std::uint8_t r = m_image.bgra[i + 2];
        const std::uint8_t a = m_image.bgra[i + 3] ? m_image.bgra[i + 3] : 255;
        premul[i + 0] = static_cast<std::uint8_t>(b * a / 255);
        premul[i + 1] = static_cast<std::uint8_t>(g * a / 255);
        premul[i + 2] = static_cast<std::uint8_t>(r * a / 255);
        premul[i + 3] = a;
    }
    D2D1_BITMAP_PROPERTIES bp = D2D1::BitmapProperties(
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
    m_rt->CreateBitmap(
        D2D1::SizeU(static_cast<UINT32>(w), static_cast<UINT32>(h)),
        premul.data(),
        static_cast<UINT32>(m_image.stride),
        bp,
        &m_bmp);

    m_w = w;
    m_h = h;
    return m_bmp != nullptr;
}

void PinWindow::ReleaseSurface()
{
    if (m_bmp)
    {
        m_bmp->Release();
        m_bmp = nullptr;
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

void PinWindow::Present()
{
    if (!m_hwnd || !EnsureSurface())
    {
        return;
    }
    RECT bind{ 0, 0, m_w, m_h };
    m_rt->BindDC(m_memDc, &bind);
    m_rt->BeginDraw();
    m_rt->Clear(D2D1::ColorF(0, 0, 0, 0));
    m_rt->DrawBitmap(m_bmp, D2D1::RectF(0, 0, static_cast<float>(m_w), static_cast<float>(m_h)));
    // Thin border for visibility.
    ID2D1SolidColorBrush* brush = nullptr;
    m_rt->CreateSolidColorBrush(D2D1::ColorF(0.2f, 0.55f, 0.95f, 1.f), &brush);
    if (brush)
    {
        m_rt->DrawRectangle(
            D2D1::RectF(0.5f, 0.5f, m_w - 0.5f, m_h - 0.5f),
            brush,
            2.f);
        brush->Release();
    }
    m_rt->EndDraw();

    RECT wnd{};
    GetWindowRect(m_hwnd, &wnd);
    POINT ptDst{ wnd.left, wnd.top };
    POINT ptSrc{ 0, 0 };
    SIZE size{ m_w, m_h };
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
