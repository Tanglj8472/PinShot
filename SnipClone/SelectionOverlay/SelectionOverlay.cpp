//==============================================================================
// SnipClone — selection on frozen capture (PinShot refine + magnifier)
//==============================================================================
#include "SelectionOverlay.h"

#include "../Common/CoordMap.h"
#include "../Common/DpiHelper.h"
#include "../Common/WinHelper.h"

#include <windowsx.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
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
constexpr LONG kMinSelectionPx = 8;
constexpr int kHandle = 6;
constexpr int kBtn = 28;
constexpr int kMagZoom = 8;
constexpr int kMagSrc = 9; // odd

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

bool PtNear(POINT p, LONG x, LONG y, LONG tol)
{
    return std::abs(p.x - x) <= tol && std::abs(p.y - y) <= tol;
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
    m_phase = Phase::Idle;
    m_dragPart = HitPart::None;
    m_model.Clear();
    m_freeze = std::move(freezeFrame);
    m_cursorValid = false;

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.style = CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS;
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

RECT SelectionOverlay::LocalSelection() const
{
    // Overlay client = freeze pixels; origin = virtual-desktop top-left (may be negative).
    const POINT origin{ m_screenOrigin.left, m_screenOrigin.top };
    const POINT tl = ScreenToImage(origin, POINT{ m_model.screenRect.left, m_model.screenRect.top });
    const POINT br = ScreenToImage(origin, POINT{ m_model.screenRect.right, m_model.screenRect.bottom });
    return RECT{ tl.x, tl.y, br.x, br.y };
}

void SelectionOverlay::SetSelectionFromLocal(const RECT& local)
{
    RECT clamped = ForceRectInBounds(local, m_clientBounds);
    if (clamped.right - clamped.left < kMinSelectionPx)
    {
        clamped.right = clamped.left + kMinSelectionPx;
    }
    if (clamped.bottom - clamped.top < kMinSelectionPx)
    {
        clamped.bottom = clamped.top + kMinSelectionPx;
    }
    clamped = ForceRectInBounds(clamped, m_clientBounds);
    const POINT origin{ m_screenOrigin.left, m_screenOrigin.top };
    const POINT tl = ImageToScreen(origin, POINT{ clamped.left, clamped.top });
    const POINT br = ImageToScreen(origin, POINT{ clamped.right, clamped.bottom });
    m_model.Set(RECT{ tl.x, tl.y, br.x, br.y });
}

void SelectionOverlay::SamplePixel(POINT client, COLORREF* outBgr, bool* ok) const
{
    *ok = false;
    if (m_freeze.Empty())
    {
        return;
    }
    if (client.x < 0 || client.y < 0 || client.x >= m_freeze.width || client.y >= m_freeze.height)
    {
        return;
    }
    const auto* px = m_freeze.bgra.data() +
                     static_cast<size_t>(client.y) * m_freeze.stride +
                     static_cast<size_t>(client.x) * 4;
    *outBgr = RGB(px[2], px[1], px[0]);
    *ok = true;
}

SelectionOverlay::HitPart SelectionOverlay::HitTest(POINT client) const
{
    if (!m_model.hasSelection)
    {
        return HitPart::None;
    }
    const RECT sel = LocalSelection();
    const LONG midX = (sel.left + sel.right) / 2;
    const LONG midY = (sel.top + sel.bottom) / 2;
    const LONG tol = ScaleForDpi(kHandle + 2, m_dpi);

    // Confirm / Cancel buttons under selection.
    const int btn = ScaleForDpi(kBtn, m_dpi);
    const int gap = ScaleForDpi(8, m_dpi);
    const LONG bx = sel.right - btn * 2 - gap;
    const LONG by = sel.bottom + gap;
    RECT confirm{ bx, by, bx + btn, by + btn };
    RECT cancel{ bx + btn + gap, by, bx + btn * 2 + gap, by + btn };
    if (PtInRect(&confirm, client))
    {
        return HitPart::Confirm;
    }
    if (PtInRect(&cancel, client))
    {
        return HitPart::Cancel;
    }

    if (PtNear(client, sel.left, sel.top, tol))
        return HitPart::NW;
    if (PtNear(client, sel.right, sel.top, tol))
        return HitPart::NE;
    if (PtNear(client, sel.left, sel.bottom, tol))
        return HitPart::SW;
    if (PtNear(client, sel.right, sel.bottom, tol))
        return HitPart::SE;
    if (PtNear(client, midX, sel.top, tol))
        return HitPart::N;
    if (PtNear(client, midX, sel.bottom, tol))
        return HitPart::S;
    if (PtNear(client, sel.left, midY, tol))
        return HitPart::W;
    if (PtNear(client, sel.right, midY, tol))
        return HitPart::E;
    if (PtInRect(&sel, client))
        return HitPart::Body;
    return HitPart::None;
}

void SelectionOverlay::UpdateCursor(HitPart hit)
{
    LPCWSTR id = IDC_CROSS;
    switch (hit)
    {
    case HitPart::N:
    case HitPart::S:
        id = IDC_SIZENS;
        break;
    case HitPart::E:
    case HitPart::W:
        id = IDC_SIZEWE;
        break;
    case HitPart::NE:
    case HitPart::SW:
        id = IDC_SIZENESW;
        break;
    case HitPart::NW:
    case HitPart::SE:
        id = IDC_SIZENWSE;
        break;
    case HitPart::Body:
        id = IDC_SIZEALL;
        break;
    case HitPart::Confirm:
    case HitPart::Cancel:
        id = IDC_HAND;
        break;
    default:
        break;
    }
    SetCursor(LoadCursorW(nullptr, id));
}

void SelectionOverlay::ApplyHandleDrag(HitPart part, POINT client)
{
    RECT r = m_dragStartSel;
    const LONG dx = client.x - m_ptStart.x;
    const LONG dy = client.y - m_ptStart.y;
    switch (part)
    {
    case HitPart::Body:
        OffsetRect(&r, dx, dy);
        break;
    case HitPart::N:
        r.top += dy;
        break;
    case HitPart::S:
        r.bottom += dy;
        break;
    case HitPart::W:
        r.left += dx;
        break;
    case HitPart::E:
        r.right += dx;
        break;
    case HitPart::NW:
        r.left += dx;
        r.top += dy;
        break;
    case HitPart::NE:
        r.right += dx;
        r.top += dy;
        break;
    case HitPart::SW:
        r.left += dx;
        r.bottom += dy;
        break;
    case HitPart::SE:
        r.right += dx;
        r.bottom += dy;
        break;
    default:
        break;
    }
    if (r.right < r.left)
    {
        std::swap(r.left, r.right);
    }
    if (r.bottom < r.top)
    {
        std::swap(r.top, r.bottom);
    }
    SetSelectionFromLocal(r);
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

    case WM_SETCURSOR:
        if (LOWORD(lParam) == HTCLIENT)
        {
            POINT pt{};
            GetCursorPos(&pt);
            ScreenToClient(hwnd, &pt);
            UpdateCursor(m_phase == Phase::Creating ? HitPart::None : HitTest(pt));
            return TRUE;
        }
        break;

    case WM_LBUTTONDOWN:
    {
        POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        if (m_phase == Phase::Adjusting && m_model.hasSelection)
        {
            const HitPart hit = HitTest(pt);
            if (hit == HitPart::Confirm)
            {
                Stop(true);
                return 0;
            }
            if (hit == HitPart::Cancel)
            {
                Stop(false);
                return 0;
            }
            if (hit != HitPart::None)
            {
                SetCapture(hwnd);
                m_dragPart = hit;
                m_ptStart = pt;
                m_dragStartSel = LocalSelection();
                return 0;
            }
        }

        SetCapture(hwnd);
        m_phase = Phase::Creating;
        m_dragPart = HitPart::None;
        m_ptStart = pt;
        m_ptCurrent = pt;
        Present();
        return 0;
    }

    case WM_MOUSEMOVE:
    {
        POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        m_cursor = pt;
        m_cursorValid = true;

        if (m_phase == Phase::Creating && GetCapture() == hwnd)
        {
            m_ptCurrent = pt;
            RECT local = RectFromPointsMinSize(m_ptStart, m_ptCurrent, kMinSelectionPx);
            SetSelectionFromLocal(local);
            Present();
            return 0;
        }
        if (m_dragPart != HitPart::None && GetCapture() == hwnd)
        {
            ApplyHandleDrag(m_dragPart, pt);
            Present();
            return 0;
        }
        Present();
        return 0;
    }

    case WM_LBUTTONUP:
    {
        if (GetCapture() == hwnd)
        {
            ReleaseCapture();
        }
        if (m_phase == Phase::Creating)
        {
            POINT pt{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
            m_ptCurrent = pt;
            RECT local = RectFromPointsMinSize(m_ptStart, m_ptCurrent, kMinSelectionPx);
            SetSelectionFromLocal(local);
            // PinShot / Snipaste: release → enter annotation immediately (no OK step).
            if (m_model.hasSelection)
            {
                Stop(true);
            }
            else
            {
                m_phase = Phase::Idle;
                Present();
            }
            return 0;
        }
        m_dragPart = HitPart::None;
        Present();
        return 0;
    }

    case WM_LBUTTONDBLCLK:
        if (m_phase == Phase::Adjusting && m_model.hasSelection)
        {
            Stop(true);
            return 0;
        }
        break;

    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE)
        {
            Stop(false);
            return 0;
        }
        if ((wParam == VK_RETURN || wParam == VK_SPACE) && m_model.hasSelection)
        {
            Stop(true);
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
                13.0f,
                L"en-us",
                &m_textFormat)))
        {
            return false;
        }
        m_textFormat->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        m_textFormat->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    }

    m_screenDc = GetDC(nullptr);
    m_memDc = CreateCompatibleDC(m_screenDc);

    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height;
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
    m_rt->CreateSolidColorBrush(D2D1::ColorF(0.15f, 0.55f, 1.f, 1.f), &m_brushAccent);

    m_bitmapW = width;
    m_bitmapH = height;

    if (!EnsureFreezeBitmap())
    {
        ReleaseSurfaceResources();
        return false;
    }
    return m_brushWhite && m_brushDim && m_brushAccent;
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
    if (m_brushAccent)
    {
        m_brushAccent->Release();
        m_brushAccent = nullptr;
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
    m_rt->DrawBitmap(m_freezeBmp, D2D1::RectF(0, 0, w, h));

    if (m_model.hasSelection)
    {
        const RECT sel = LocalSelection();
        const float l = static_cast<float>(sel.left);
        const float t = static_cast<float>(sel.top);
        const float r = static_cast<float>(sel.right);
        const float b = static_cast<float>(sel.bottom);

        if (t > 0.f)
            m_rt->FillRectangle(D2D1::RectF(0, 0, w, t), m_brushDim);
        if (b < h)
            m_rt->FillRectangle(D2D1::RectF(0, b, w, h), m_brushDim);
        if (l > 0.f && b > t)
            m_rt->FillRectangle(D2D1::RectF(0, t, l, b), m_brushDim);
        if (r < w && b > t)
            m_rt->FillRectangle(D2D1::RectF(r, t, w, b), m_brushDim);

        const float stroke = static_cast<float>(ScaleForDpi(2, m_dpi));
        m_rt->DrawRectangle(D2D1::RectF(l, t, r, b), m_brushWhite, stroke);

        // 8 handles
        const float hs = static_cast<float>(ScaleForDpi(kHandle, m_dpi));
        const float midx = (l + r) * 0.5f;
        const float midy = (t + b) * 0.5f;
        const D2D1_POINT_2F handles[] = {
            { l, t }, { midx, t }, { r, t }, { r, midy }, { r, b }, { midx, b }, { l, b }, { l, midy }
        };
        for (const auto& hp : handles)
        {
            m_rt->FillRectangle(
                D2D1::RectF(hp.x - hs, hp.y - hs, hp.x + hs, hp.y + hs),
                m_brushAccent);
            m_rt->DrawRectangle(
                D2D1::RectF(hp.x - hs, hp.y - hs, hp.x + hs, hp.y + hs),
                m_brushWhite,
                1.f);
        }

        // Size label
        if (m_textFormat)
        {
            wchar_t label[96]{};
            swprintf_s(
                label,
                L"%ld × %ld   Enter确认  Esc取消",
                m_model.Width(),
                m_model.Height());
            ID2D1SolidColorBrush* bg = nullptr;
            m_rt->CreateSolidColorBrush(D2D1::ColorF(0.f, 0.f, 0.f, 0.75f), &bg);
            const float labelW = 260.f;
            const float labelH = 22.f;
            float lx = l;
            float ly = t - labelH - 6.f;
            if (ly < 4.f)
            {
                ly = t + 6.f;
            }
            if (bg)
            {
                m_rt->FillRectangle(D2D1::RectF(lx, ly, lx + labelW, ly + labelH), bg);
                m_rt->DrawText(
                    label,
                    static_cast<UINT32>(wcslen(label)),
                    m_textFormat,
                    D2D1::RectF(lx + 8.f, ly, lx + labelW, ly + labelH),
                    m_brushWhite);
                bg->Release();
            }
        }

        // Confirm / Cancel buttons
        if (m_phase == Phase::Adjusting)
        {
            const float btn = static_cast<float>(ScaleForDpi(kBtn, m_dpi));
            const float gap = static_cast<float>(ScaleForDpi(8, m_dpi));
            const float bx = r - btn * 2.f - gap;
            const float by = b + gap;
            m_rt->FillRectangle(D2D1::RectF(bx, by, bx + btn, by + btn), m_brushAccent);
            m_rt->FillRectangle(
                D2D1::RectF(bx + btn + gap, by, bx + btn * 2.f + gap, by + btn),
                m_brushDim);
            if (m_textFormat)
            {
                m_rt->DrawText(
                    L"OK",
                    2,
                    m_textFormat,
                    D2D1::RectF(bx, by, bx + btn, by + btn),
                    m_brushWhite);
                m_rt->DrawText(
                    L"X",
                    1,
                    m_textFormat,
                    D2D1::RectF(bx + btn + gap, by, bx + btn * 2.f + gap, by + btn),
                    m_brushWhite);
            }
        }
    }
    else
    {
        m_rt->FillRectangle(D2D1::RectF(0, 0, w, h), m_brushDim);
    }

    // Magnifier + HEX
    if (m_cursorValid && m_phase != Phase::Creating)
    {
        COLORREF bgr{};
        bool ok = false;
        SamplePixel(m_cursor, &bgr, &ok);
        const float mag = static_cast<float>(kMagSrc * kMagZoom);
        float mx = static_cast<float>(m_cursor.x) + 24.f;
        float my = static_cast<float>(m_cursor.y) + 24.f;
        if (mx + mag + 8.f > w)
        {
            mx = static_cast<float>(m_cursor.x) - mag - 24.f;
        }
        if (my + mag + 28.f > h)
        {
            my = static_cast<float>(m_cursor.y) - mag - 40.f;
        }

        // Build magnified patch from freeze CPU buffer.
        const int half = kMagSrc / 2;
        std::vector<std::uint8_t> patch(static_cast<size_t>(kMagSrc * kMagSrc * 4));
        for (int y = 0; y < kMagSrc; ++y)
        {
            for (int x = 0; x < kMagSrc; ++x)
            {
                int sx = m_cursor.x - half + x;
                int sy = m_cursor.y - half + y;
                sx = (std::max)(0, (std::min)(m_freeze.width - 1, sx));
                sy = (std::max)(0, (std::min)(m_freeze.height - 1, sy));
                const auto* src = m_freeze.bgra.data() +
                                  static_cast<size_t>(sy) * m_freeze.stride +
                                  static_cast<size_t>(sx) * 4;
                auto* dst = patch.data() + static_cast<size_t>((y * kMagSrc + x) * 4);
                dst[0] = src[0];
                dst[1] = src[1];
                dst[2] = src[2];
                dst[3] = 255;
            }
        }
        D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
            D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
        ID2D1Bitmap* magBmp = nullptr;
        m_rt->CreateBitmap(
            D2D1::SizeU(kMagSrc, kMagSrc),
            patch.data(),
            static_cast<UINT32>(kMagSrc * 4),
            props,
            &magBmp);
        if (magBmp)
        {
            m_rt->DrawBitmap(
                magBmp,
                D2D1::RectF(mx, my, mx + mag, my + mag),
                1.f,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
            magBmp->Release();
        }
        m_rt->DrawRectangle(D2D1::RectF(mx, my, mx + mag, my + mag), m_brushWhite, 1.f);
        // Crosshair center
        const float cx = mx + mag * 0.5f;
        const float cy = my + mag * 0.5f;
        m_rt->DrawLine(D2D1::Point2F(cx - 6, cy), D2D1::Point2F(cx + 6, cy), m_brushAccent, 1.f);
        m_rt->DrawLine(D2D1::Point2F(cx, cy - 6), D2D1::Point2F(cx, cy + 6), m_brushAccent, 1.f);

        if (ok && m_textFormat)
        {
            wchar_t hex[32]{};
            swprintf_s(hex, L"#%02X%02X%02X", GetRValue(bgr), GetGValue(bgr), GetBValue(bgr));
            ID2D1SolidColorBrush* bg = nullptr;
            m_rt->CreateSolidColorBrush(D2D1::ColorF(0, 0, 0, 0.8f), &bg);
            if (bg)
            {
                m_rt->FillRectangle(D2D1::RectF(mx, my + mag + 2.f, mx + mag, my + mag + 22.f), bg);
                bg->Release();
            }
            m_rt->DrawText(
                hex,
                static_cast<UINT32>(wcslen(hex)),
                m_textFormat,
                D2D1::RectF(mx + 6.f, my + mag + 2.f, mx + mag, my + mag + 22.f),
                m_brushWhite);
        }
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
