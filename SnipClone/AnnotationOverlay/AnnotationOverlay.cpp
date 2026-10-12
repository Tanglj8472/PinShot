//==============================================================================
// SnipClone — annotation host
// 文字交互目标：Snipaste / PinShot — 落点浮动 EDIT、Enter 提交、Esc 取消、双击重编
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

    // Must have a visible freeze crop before the toolbar appears (blank-host bug).
    if (!EnsureSurface())
    {
        DestroyWindow(m_hwnd);
        m_hwnd = nullptr;
        ReleaseSurface();
        return false;
    }

    m_ann.SetChangedCallback([this]() {
        Present();
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
        }
    });
    m_ann.SetTool(ToolKind::Pen);

    ShowWindow(m_hwnd, SW_SHOW);
    SetForegroundWindow(m_hwnd);
    Present();

    m_toolbar = std::make_unique<Toolbar>();
    m_toolbar->Create(instance, m_hwnd, &m_ann, [this](ToolbarAction a, ToolKind t) {
        OnToolbarAction(a, t);
    });
    m_toolbar->SyncFromAnnotator();
    m_toolbar->PlaceBelow(m_base.screenRect);

    m_textEditor = std::make_unique<TextEditor>();
    return true;
}

void AnnotationOverlay::Stop()
{
    if (m_textEditor)
    {
        if (m_textEditor->IsOpen())
        {
            m_textEditor->Close(false);
        }
        m_textEditor->Destroy();
        m_textEditor.reset();
    }
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

void AnnotationOverlay::OpenTextEditorAt(D2D1_POINT_2F imagePt, int editIndex)
{
    if (!m_textEditor)
    {
        m_textEditor = std::make_unique<TextEditor>();
    }
    if (m_textEditor->IsOpen())
    {
        m_textEditor->Close(true); // commit previous like PinShot click-away
    }

    m_editingTextIndex = editIndex;
    m_textAnchor = imagePt;

    std::wstring initial;
    DrawStyle style = m_ann.Style(ToolKind::Text);
    if (editIndex >= 0)
    {
        if (const auto* sh = m_ann.ShapeAt(editIndex))
        {
            initial = sh->text;
            style = sh->style;
            // Keep tool style in sync for B/I/OL toggles while editing.
            m_ann.MutableStyle(ToolKind::Text) = style;
        }
    }

    POINT screen{
        m_base.screenRect.left + static_cast<LONG>(imagePt.x),
        m_base.screenRect.top + static_cast<LONG>(imagePt.y)
    };

    m_textEditor->Open(
        m_instance,
        m_hwnd,
        screen,
        style,
        initial,
        [this](bool committed, const std::wstring& text) { OnTextEditorDone(committed, text); });

    if (m_toolbar)
    {
        m_toolbar->SyncFromAnnotator();
    }
}

void AnnotationOverlay::OnTextEditorDone(bool committed, const std::wstring& text)
{
    const int idx = m_editingTextIndex;
    m_editingTextIndex = -1;

    if (committed)
    {
        if (idx >= 0)
        {
            m_ann.UpdateText(idx, text);
        }
        else
        {
            m_ann.AddText(m_textAnchor, text);
        }
    }

    if (m_hwnd)
    {
        SetFocus(m_hwnd);
    }
    Present();
}

LRESULT AnnotationOverlay::WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_LBUTTONDOWN:
    {
        const D2D1_POINT_2F pt = D2D1::Point2F(
            static_cast<float>(GET_X_LPARAM(lParam)),
            static_cast<float>(GET_Y_LPARAM(lParam)));

        if (m_ann.Tool() == ToolKind::Text)
        {
            // PinShot: click existing text → re-edit; blank → place new.
            // If editor already open, first commit then return (second click places new).
            if (m_textEditor && m_textEditor->IsOpen())
            {
                m_textEditor->Close(true);
                return 0;
            }
            const int hit = m_ann.HitTest(pt);
            if (hit >= 0)
            {
                if (const auto* sh = m_ann.ShapeAt(hit))
                {
                    OpenTextEditorAt(sh->pts[0], hit);
                }
            }
            else
            {
                OpenTextEditorAt(pt, -1);
            }
            return 0;
        }

        SetCapture(hwnd);
        m_ann.Press(pt);
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        return 0;
    }
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

    case WM_LBUTTONDBLCLK:
    {
        // Snipaste/PinShot: double-click text to re-edit (even if not on text tool).
        const D2D1_POINT_2F pt = D2D1::Point2F(
            static_cast<float>(GET_X_LPARAM(lParam)),
            static_cast<float>(GET_Y_LPARAM(lParam)));
        const int hit = m_ann.HitTest(pt);
        if (hit >= 0)
        {
            if (const auto* sh = m_ann.ShapeAt(hit))
            {
                m_ann.SetTool(ToolKind::Text);
                if (m_toolbar)
                {
                    m_toolbar->SyncFromAnnotator();
                }
                OpenTextEditorAt(sh->pts[0], hit);
            }
            return 0;
        }
        break;
    }

    case WM_SNIP_CLOSE_SELF:
        Stop();
        return 0;

    case WM_KEYDOWN:
        // Text editor owns Esc/Enter while open.
        if (m_textEditor && m_textEditor->IsOpen())
        {
            return 0;
        }
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
        if (!(GetKeyState(VK_CONTROL) & 0x8000))
        {
            auto setTool = [this](ToolKind k) {
                if (m_textEditor && m_textEditor->IsOpen())
                {
                    m_textEditor->Close(true);
                }
                m_ann.SetTool(k);
                if (m_toolbar)
                {
                    m_toolbar->SyncFromAnnotator();
                }
            };
            switch (wParam)
            {
            case 'P':
                setTool(ToolKind::Pen);
                return 0;
            case 'R':
                setTool(ToolKind::Rect);
                return 0;
            case 'E':
                setTool(ToolKind::Ellipse);
                return 0;
            case 'A':
                setTool(ToolKind::Arrow);
                return 0;
            case 'X':
                setTool(ToolKind::Text);
                return 0;
            case 'M':
                setTool(ToolKind::Mosaic);
                return 0;
            case '1':
                if (m_ann.Tool() == ToolKind::Text)
                {
                    m_ann.AdjustFontSize(-1);
                    if (m_textEditor && m_textEditor->IsOpen())
                    {
                        m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
                    }
                }
                else if (m_ann.Tool() == ToolKind::Mosaic)
                    m_ann.AdjustMosaicSize(-1);
                else
                    m_ann.AdjustWidth(-1);
                return 0;
            case '2':
                if (m_ann.Tool() == ToolKind::Text)
                {
                    m_ann.AdjustFontSize(+1);
                    if (m_textEditor && m_textEditor->IsOpen())
                    {
                        m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
                    }
                }
                else if (m_ann.Tool() == ToolKind::Mosaic)
                    m_ann.AdjustMosaicSize(+1);
                else
                    m_ann.AdjustWidth(+1);
                return 0;
            default:
                break;
            }
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
    if (m_textEditor && m_textEditor->IsOpen())
    {
        m_textEditor->Close(false);
    }
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
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->Close(false);
        }
        m_ann.Undo();
        break;
    case ToolbarAction::Redo:
        m_ann.Redo();
        break;
    case ToolbarAction::WidthDec:
        if (m_ann.Tool() == ToolKind::Text)
        {
            m_ann.AdjustFontSize(-1);
            if (m_textEditor && m_textEditor->IsOpen())
            {
                m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
            }
        }
        else if (m_ann.Tool() == ToolKind::Mosaic)
            m_ann.AdjustMosaicSize(-1);
        else
            m_ann.AdjustWidth(-1);
        break;
    case ToolbarAction::WidthInc:
        if (m_ann.Tool() == ToolKind::Text)
        {
            m_ann.AdjustFontSize(+1);
            if (m_textEditor && m_textEditor->IsOpen())
            {
                m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
            }
        }
        else if (m_ann.Tool() == ToolKind::Mosaic)
            m_ann.AdjustMosaicSize(+1);
        else
            m_ann.AdjustWidth(+1);
        break;
    case ToolbarAction::ColorCycle:
        m_ann.CycleColor();
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
        }
        break;
    case ToolbarAction::Bold:
        m_ann.ToggleBold();
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
        }
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        break;
    case ToolbarAction::Italic:
        m_ann.ToggleItalic();
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->ApplyStyle(m_ann.Style(ToolKind::Text));
        }
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        break;
    case ToolbarAction::Outline:
        m_ann.ToggleOutline();
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        Present();
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
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->Close(true);
        }
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
    if (m_textEditor && m_textEditor->IsOpen())
    {
        m_textEditor->Close(true);
    }
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
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
        96.f,
        96.f);
    if (FAILED(m_renderer.Factory()->CreateDCRenderTarget(&props, &m_rt)))
    {
        return false;
    }
    RECT bind{ 0, 0, w, h };
    if (FAILED(m_rt->BindDC(m_memDc, &bind)))
    {
        return false;
    }
    m_baseBmp = m_renderer.CreateBitmapFromCapture(m_rt, m_base);
    if (!m_baseBmp)
    {
        return false;
    }
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
    if (FAILED(m_rt->BindDC(m_memDc, &bind)))
    {
        return;
    }
    m_rt->BeginDraw();
    m_rt->Clear(D2D1::ColorF(0, 0, 0, 0));
    m_renderer.DrawScene(m_rt, m_baseBmp, &m_base, m_ann);
    const HRESULT hr = m_rt->EndDraw();
    if (FAILED(hr))
    {
        if (hr == D2DERR_RECREATE_TARGET)
        {
            ReleaseSurface();
        }
        return;
    }

    POINT ptSrc{ 0, 0 };
    SIZE size{ m_w, m_h };
    POINT ptDst{ m_base.screenRect.left, m_base.screenRect.top };
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
