//==============================================================================
// SnipClone — annotation host
//==============================================================================
#include "AnnotationOverlay.h"

#include "../Common/ImageUtil.h"
#include "../Common/WinHelper.h"
#include "../PinWindow/PinManager.h"

#include <windowsx.h>

namespace snip
{

CaptureResult AnnotationOverlay::s_lastExport{};
POINT AnnotationOverlay::s_lastOrigin{};

AnnotationOverlay::~AnnotationOverlay()
{
    Stop();
}

const CaptureResult& AnnotationOverlay::LastExport()
{
    return s_lastExport;
}

POINT AnnotationOverlay::LastExportOrigin()
{
    return s_lastOrigin;
}

bool AnnotationOverlay::Start(HINSTANCE instance, HWND owner, CaptureResult image)
{
    if (m_hwnd || image.Empty())
    {
        return false;
    }

    m_instance = instance;
    m_owner = owner;
    m_base = std::move(image);

    if (!m_renderer.Initialize())
    {
        return false;
    }

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = StaticWndProc;
    wc.hInstance = instance;
    wc.hCursor = LoadCursorW(nullptr, IDC_CROSS);
    wc.lpszClassName = L"SnipCloneAnnotationOverlay";
    EnsureWindowClass(wc);

    m_hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        L"SnipCloneAnnotationOverlay",
        L"",
        WS_POPUP,
        m_base.screenRect.left,
        m_base.screenRect.top,
        m_base.width,
        m_base.height,
        owner,
        nullptr,
        instance,
        this);
    if (!m_hwnd)
    {
        return false;
    }

    SetWindowDisplayAffinity(m_hwnd, WDA_EXCLUDEFROMCAPTURE);

    m_ann.SetChangedCallback([this]() { Present(); });
    m_ann.SetTool(ToolKind::Pen);

    m_toolbar = std::make_unique<Toolbar>();
    m_toolbar->Create(instance, m_hwnd, &m_ann, [this](ToolbarAction a, ToolKind t) {
        OnToolbarAction(a, t);
    });
    m_toolbar->SyncFromAnnotator();
    m_toolbar->PlaceBelow(m_base.screenRect);

    ShowWindow(m_hwnd, SW_SHOW);
    SetForegroundWindow(m_hwnd);
    Present();
    return true;
}

void AnnotationOverlay::Stop()
{
    if (m_toolbar)
    {
        m_toolbar->Destroy();
        m_toolbar.reset();
    }
    if (m_hwnd)
    {
        HWND hwnd = m_hwnd;
        m_hwnd = nullptr;
        DestroyWindow(hwnd);
    }
    ReleaseSurface();
    if (m_baseBmp)
    {
        m_baseBmp->Release();
        m_baseBmp = nullptr;
    }
    m_renderer.Shutdown();
}

LRESULT CALLBACK AnnotationOverlay::StaticWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        auto* self = static_cast<AnnotationOverlay*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        self->m_hwnd = hwnd;
        return TRUE;
    }
    auto* self = reinterpret_cast<AnnotationOverlay*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    return self ? self->WndProc(hwnd, msg, wParam, lParam) : DefWindowProcW(hwnd, msg, wParam, lParam);
}

LRESULT AnnotationOverlay::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_LBUTTONDOWN:
        SetCapture(hwnd);
        m_ann.Press(D2D1::Point2F(
            static_cast<float>(GET_X_LPARAM(lParam)),
            static_cast<float>(GET_Y_LPARAM(lParam))));
        return 0;
    case WM_MOUSEMOVE:
        if (GetCapture() == hwnd)
        {
            m_ann.Move(D2D1::Point2F(
                static_cast<float>(GET_X_LPARAM(lParam)),
                static_cast<float>(GET_Y_LPARAM(lParam))));
        }
        return 0;
    case WM_LBUTTONUP:
        if (GetCapture() == hwnd)
        {
            ReleaseCapture();
        }
        m_ann.Release(D2D1::Point2F(
            static_cast<float>(GET_X_LPARAM(lParam)),
            static_cast<float>(GET_Y_LPARAM(lParam))));
        return 0;
    case WM_SNIP_CLOSE_SELF:
        Stop();
        return 0;

    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE)
        {
            RequestClose();
            return 0;
        }
        if (wParam == 'Z' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            m_ann.Undo();
            return 0;
        }
        if (wParam == 'Y' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            m_ann.Redo();
            return 0;
        }
        if (wParam == 'C' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            DoCopy();
            return 0;
        }
        if (wParam == 'S' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            DoSave();
            return 0;
        }
        if (wParam == 'T' && (GetKeyState(VK_CONTROL) & 0x8000))
        {
            DoPin();
            return 0;
        }
        break;
    case WM_DESTROY:
        ReleaseSurface();
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        if (m_owner)
        {
            PostMessageW(m_owner, WM_SNIP_ANNOTATION_DONE, 0, 0);
        }
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

void AnnotationOverlay::RequestClose()
{
    if (m_hwnd)
    {
        PostMessageW(m_hwnd, WM_SNIP_CLOSE_SELF, 0, 0);
    }
}

void AnnotationOverlay::OnToolbarAction(ToolbarAction action, ToolKind /*tool*/)
{
    switch (action)
    {
    case ToolbarAction::Undo:
        m_ann.Undo();
        break;
    case ToolbarAction::Redo:
        m_ann.Redo();
        break;
    case ToolbarAction::Copy:
        DoCopy();
        break;
    case ToolbarAction::Save:
        DoSave();
        break;
    case ToolbarAction::Pin:
        DoPin();
        break;
    case ToolbarAction::Close:
        RequestClose();
        break;
    case ToolbarAction::SetTool:
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        break;
    default:
        break;
    }
}

CaptureResult AnnotationOverlay::ComposeExport()
{
    m_ann.Finish();
    return m_renderer.Compose(m_base, m_ann);
}

void AnnotationOverlay::DoCopy()
{
    auto composed = ComposeExport();
    if (ImageUtil::CopyToClipboard(composed))
    {
        s_lastExport = std::move(composed);
        s_lastOrigin = { m_base.screenRect.left, m_base.screenRect.top };
        RequestClose();
    }
}

void AnnotationOverlay::DoSave()
{
    auto path = ImageUtil::BrowseSavePng(m_hwnd);
    if (path.empty())
    {
        return;
    }
    auto composed = ComposeExport();
    if (ImageUtil::SavePng(composed, path))
    {
        s_lastExport = std::move(composed);
        s_lastOrigin = { m_base.screenRect.left, m_base.screenRect.top };
        RequestClose();
    }
}

void AnnotationOverlay::DoPin()
{
    auto composed = ComposeExport();
    s_lastExport = composed;
    s_lastOrigin = { m_base.screenRect.left, m_base.screenRect.top };
    PinManager::Instance().CreatePin(m_instance, composed, s_lastOrigin);
    RequestClose();
}

bool AnnotationOverlay::EnsureSurface()
{
    const int w = m_base.width;
    const int h = m_base.height;
    if (w <= 0 || h <= 0)
    {
        return false;
    }
    if (m_rt && m_w == w && m_h == h)
    {
        return true;
    }
    ReleaseSurface();

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
    if (FAILED(m_renderer.Factory()->CreateDCRenderTarget(&props, &m_rt)))
    {
        return false;
    }
    RECT bind{ 0, 0, w, h };
    m_rt->BindDC(m_memDc, &bind);
    m_baseBmp = m_renderer.CreateBitmapFromCapture(m_rt, m_base);
    m_w = w;
    m_h = h;
    return true;
}

void AnnotationOverlay::ReleaseSurface()
{
    if (m_baseBmp)
    {
        m_baseBmp->Release();
        m_baseBmp = nullptr;
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

void AnnotationOverlay::Present()
{
    if (!m_hwnd || !EnsureSurface())
    {
        return;
    }
    RECT bind{ 0, 0, m_w, m_h };
    m_rt->BindDC(m_memDc, &bind);
    m_rt->BeginDraw();
    m_rt->Clear(D2D1::ColorF(0, 0, 0, 0));
    m_renderer.DrawScene(m_rt, m_baseBmp, m_ann);
    m_rt->EndDraw();

    POINT ptSrc{ 0, 0 };
    SIZE size{ m_w, m_h };
    POINT ptDst{ m_base.screenRect.left, m_base.screenRect.top };
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
