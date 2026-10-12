//==============================================================================
// SnipClone — pin window
//==============================================================================
#include "PinWindow.h"

#include "PinManager.h"
#include "../Common/AppSettings.h"
#include "../Common/ImageUtil.h"
#include "../Common/WinHelper.h"

#include <windowsx.h>

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

#pragma comment(lib, "d2d1.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipClonePinWindow";
constexpr float kMinScale = 0.15f;
constexpr float kMaxScale = 8.f;
}

PinWindow::~PinWindow()
{
    ReleaseSurface();
    if (m_factory)
    {
        m_factory->Release();
        m_factory = nullptr;
    }
}

int PinWindow::DisplayWidth() const
{
    return (std::max)(1, static_cast<int>(std::lround(m_image.width * m_scale)));
}

int PinWindow::DisplayHeight() const
{
    return (std::max)(1, static_cast<int>(std::lround(m_image.height * m_scale)));
}

bool PinWindow::Create(HINSTANCE instance, CaptureResult image, POINT screenOrigin)
{
    PinSnapshot snap;
    snap.image = std::move(image);
    snap.origin = screenOrigin;
    return CreateFromSnapshot(instance, std::move(snap));
}

bool PinWindow::CreateFromSnapshot(HINSTANCE instance, PinSnapshot snap)
{
    if (m_hwnd || snap.image.Empty())
    {
        return false;
    }
    m_instance = instance;
    m_image = std::move(snap.image);
    m_origin = snap.origin;
    m_scale = (std::max)(kMinScale, (std::min)(kMaxScale, snap.scale <= 0.f ? 1.f : snap.scale));
    m_opacity = snap.opacity ? snap.opacity : 255;

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
        m_origin.x,
        m_origin.y,
        DisplayWidth(),
        DisplayHeight(),
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

PinSnapshot PinWindow::Snapshot() const
{
    PinSnapshot s;
    s.image = m_image;
    s.origin = m_origin;
    if (m_hwnd)
    {
        RECT rc{};
        GetWindowRect(m_hwnd, &rc);
        s.origin = { rc.left, rc.top };
    }
    s.scale = m_scale;
    s.opacity = m_opacity;
    return s;
}

void PinWindow::SetScale(float scale)
{
    m_scale = (std::max)(kMinScale, (std::min)(kMaxScale, scale));
    ApplyWindowSize();
    Present();
}

void PinWindow::ResetScale()
{
    SetScale(1.f);
}

void PinWindow::SetOpacity(BYTE opacity)
{
    m_opacity = opacity ? opacity : 1;
    Present();
}

void PinWindow::SetClickThrough(bool enabled)
{
    if (!m_hwnd || m_clickThrough == enabled)
    {
        return;
    }
    m_clickThrough = enabled;
    LONG_PTR ex = GetWindowLongPtrW(m_hwnd, GWL_EXSTYLE);
    if (enabled)
    {
        ex |= WS_EX_TRANSPARENT;
    }
    else
    {
        ex &= ~WS_EX_TRANSPARENT;
    }
    SetWindowLongPtrW(m_hwnd, GWL_EXSTYLE, ex);
    // Force style refresh for layered hit-testing.
    SetWindowPos(
        m_hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    Present();
}

void PinWindow::SetVisible(bool visible)
{
    if (m_hwnd)
    {
        ShowWindow(m_hwnd, visible ? SW_SHOWNA : SW_HIDE);
    }
}

bool PinWindow::IsVisible() const
{
    return m_hwnd && IsWindowVisible(m_hwnd);
}

void PinWindow::CopyImage()
{
    if (!ImageUtil::CopyToClipboard(m_image))
    {
        MessageBoxW(m_hwnd, L"复制到剪贴板失败。", L"SnipClone", MB_ICONWARNING | MB_OK);
    }
}

void PinWindow::SaveImage()
{
    auto settings = AppSettings::Load();
    auto path = ImageUtil::BrowseSavePng(m_hwnd, settings.outputDir);
    if (path.empty())
    {
        return;
    }
    if (!ImageUtil::SavePng(m_image, path))
    {
        MessageBoxW(m_hwnd, L"保存 PNG 失败。", L"SnipClone", MB_ICONWARNING | MB_OK);
        return;
    }
    const auto slash = path.find_last_of(L"\\/");
    if (slash != std::wstring::npos)
    {
        settings.outputDir = path.substr(0, slash);
        settings.Save();
    }
}

void PinWindow::ApplyWindowSize()
{
    if (!m_hwnd)
    {
        return;
    }
    RECT rc{};
    GetWindowRect(m_hwnd, &rc);
    m_origin = { rc.left, rc.top };
    SetWindowPos(
        m_hwnd,
        HWND_TOPMOST,
        m_origin.x,
        m_origin.y,
        DisplayWidth(),
        DisplayHeight(),
        SWP_NOACTIVATE);
    ReleaseSurface();
}

void PinWindow::ShowContextMenu(POINT screen)
{
    HMENU menu = CreatePopupMenu();
    if (!menu)
    {
        return;
    }
    AppendMenuW(menu, MF_STRING, IDM_COPY, L"复制");
    AppendMenuW(menu, MF_STRING, IDM_SAVE, L"保存 PNG…");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, IDM_RESET_SCALE, L"恢复原始比例");
    HMENU opacity = CreatePopupMenu();
    AppendMenuW(opacity, MF_STRING, IDM_OPACITY_100, L"100%");
    AppendMenuW(opacity, MF_STRING, IDM_OPACITY_75, L"75%");
    AppendMenuW(opacity, MF_STRING, IDM_OPACITY_50, L"50%");
    AppendMenuW(opacity, MF_STRING, IDM_OPACITY_25, L"25%");
    AppendMenuW(menu, MF_POPUP, reinterpret_cast<UINT_PTR>(opacity), L"透明度");
    AppendMenuW(
        menu,
        MF_STRING | (m_clickThrough ? MF_CHECKED : 0),
        IDM_CLICK_THROUGH,
        L"鼠标穿透（Ctrl+Shift+P 退出）");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, IDM_HIDE_ALL, L"隐藏全部贴图");
    AppendMenuW(menu, MF_STRING, IDM_SHOW_ALL, L"显示全部贴图");
    AppendMenuW(
        menu,
        MF_STRING | (PinManager::Instance().HasLastClosed() ? 0 : MF_GRAYED),
        IDM_RESTORE_LAST,
        L"恢复最近关闭");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, IDM_CLOSE, L"关闭");

    const UINT cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        screen.x,
        screen.y,
        0,
        m_hwnd,
        nullptr);
    DestroyMenu(menu);
    if (!cmd)
    {
        return;
    }

    switch (cmd)
    {
    case IDM_COPY:
        CopyImage();
        break;
    case IDM_SAVE:
        SaveImage();
        break;
    case IDM_RESET_SCALE:
        ResetScale();
        break;
    case IDM_OPACITY_100:
        SetOpacity(255);
        break;
    case IDM_OPACITY_75:
        SetOpacity(191);
        break;
    case IDM_OPACITY_50:
        SetOpacity(128);
        break;
    case IDM_OPACITY_25:
        SetOpacity(64);
        break;
    case IDM_CLICK_THROUGH:
        SetClickThrough(!m_clickThrough);
        break;
    case IDM_HIDE_ALL:
        PinManager::Instance().HideAll();
        break;
    case IDM_SHOW_ALL:
        PinManager::Instance().ShowAll();
        break;
    case IDM_RESTORE_LAST:
        PinManager::Instance().RestoreLastClosed(m_instance);
        break;
    case IDM_CLOSE:
        PinManager::Instance().RequestClose(this);
        break;
    default:
        break;
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
    case WM_MOUSEACTIVATE:
        if (!m_clickThrough)
        {
            SetFocus(hwnd);
        }
        return MA_ACTIVATE;
    case WM_LBUTTONDOWN:
        if (!m_clickThrough)
        {
            SetFocus(hwnd);
            SetCapture(hwnd);
            m_dragging = true;
            GetCursorPos(&m_dragScreen);
            RECT rc{};
            GetWindowRect(hwnd, &rc);
            m_dragOrigin = { rc.left, rc.top };
        }
        return 0;
    case WM_MOUSEMOVE:
        if (m_dragging && (wParam & MK_LBUTTON))
        {
            POINT cur{};
            GetCursorPos(&cur);
            const int nx = m_dragOrigin.x + (cur.x - m_dragScreen.x);
            const int ny = m_dragOrigin.y + (cur.y - m_dragScreen.y);
            SetWindowPos(hwnd, HWND_TOPMOST, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            m_origin = { nx, ny };
        }
        return 0;
    case WM_LBUTTONUP:
        if (m_dragging)
        {
            m_dragging = false;
            if (GetCapture() == hwnd)
            {
                ReleaseCapture();
            }
            RECT rc{};
            GetWindowRect(hwnd, &rc);
            m_origin = { rc.left, rc.top };
        }
        return 0;
    case WM_RBUTTONUP:
    case WM_CONTEXTMENU:
    {
        if (m_clickThrough)
        {
            return 0;
        }
        POINT pt{};
        if (msg == WM_CONTEXTMENU && GET_X_LPARAM(lParam) != -1)
        {
            pt.x = GET_X_LPARAM(lParam);
            pt.y = GET_Y_LPARAM(lParam);
        }
        else
        {
            GetCursorPos(&pt);
        }
        ShowContextMenu(pt);
        return 0;
    }
    case WM_MOUSEWHEEL:
    {
        if (m_clickThrough)
        {
            return 0;
        }
        const int delta = GET_WHEEL_DELTA_WPARAM(wParam);
        const float factor = delta > 0 ? 1.1f : (1.f / 1.1f);
        // Zoom about window center; keep center stable on screen.
        RECT rc{};
        GetWindowRect(hwnd, &rc);
        const int cx = (rc.left + rc.right) / 2;
        const int cy = (rc.top + rc.bottom) / 2;
        const float newScale = m_scale * factor;
        m_scale = (std::max)(kMinScale, (std::min)(kMaxScale, newScale));
        const int nw = DisplayWidth();
        const int nh = DisplayHeight();
        m_origin = { cx - nw / 2, cy - nh / 2 };
        SetWindowPos(hwnd, HWND_TOPMOST, m_origin.x, m_origin.y, nw, nh, SWP_NOACTIVATE);
        ReleaseSurface();
        Present();
        return 0;
    }
    case WM_LBUTTONDBLCLK:
        PinManager::Instance().RequestClose(this);
        return 0;
    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE || wParam == VK_DELETE)
        {
            PinManager::Instance().RequestClose(this);
            return 0;
        }
        if (wParam == 'C' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            CopyImage();
            return 0;
        }
        if (wParam == 'S' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            SaveImage();
            return 0;
        }
        if (wParam == '0' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            ResetScale();
            return 0;
        }
        break;
    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;
    case WM_DESTROY:
        m_hwnd = nullptr;
        ReleaseSurface();
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return 0;
    case WM_NCDESTROY:
        PinManager::Instance().OnNcDestroy(this);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

bool PinWindow::EnsureSurface()
{
    const int w = DisplayWidth();
    const int h = DisplayHeight();
    if (m_rt && m_bmp && m_w == w && m_h == h)
    {
        return true;
    }
    ReleaseSurface();
    if (!m_factory)
    {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &m_factory);
    }
    if (!m_factory || m_image.Empty())
    {
        return false;
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
    if (FAILED(m_factory->CreateDCRenderTarget(&props, &m_rt)) || !m_rt)
    {
        return false;
    }
    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);

    // Upload source at native size; Present scales via DrawBitmap dest rect.
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
        D2D1::SizeU(static_cast<UINT32>(m_image.width), static_cast<UINT32>(m_image.height)),
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

    // Border: blue normal, orange when click-through (visible cue).
    const D2D1_COLOR_F border = m_clickThrough
                                    ? D2D1::ColorF(1.f, 0.45f, 0.1f, 1.f)
                                    : D2D1::ColorF(0.2f, 0.55f, 0.95f, 1.f);
    ID2D1SolidColorBrush* brush = nullptr;
    m_rt->CreateSolidColorBrush(border, &brush);
    if (brush)
    {
        m_rt->DrawRectangle(
            D2D1::RectF(0.5f, 0.5f, m_w - 0.5f, m_h - 0.5f),
            brush,
            m_clickThrough ? 3.f : 2.f);
        brush->Release();
    }
    m_rt->EndDraw();

    RECT wnd{};
    GetWindowRect(m_hwnd, &wnd);
    POINT ptDst{ wnd.left, wnd.top };
    POINT ptSrc{ 0, 0 };
    SIZE size{ m_w, m_h };
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, m_opacity, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
