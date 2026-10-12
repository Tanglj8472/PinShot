//==============================================================================
// SnipClone — annotation host
// 文字交互目标：Snipaste / PinShot — 落点浮动 EDIT、Enter 提交、Esc 取消、双击重编
//==============================================================================
#include "AnnotationOverlay.h"

#include "../Common/AppSettings.h"
#include "../Common/CoordMap.h"
#include "../Common/ImageUtil.h"
#include "../Common/WinHelper.h"
#include "../PinWindow/PinManager.h"

#include <windowsx.h>

#include <algorithm>
#include <string>

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

bool AnnotationOverlay::Start(HINSTANCE instance, HWND owner, CaptureResult crop, CaptureResult freeze)
{
    if (m_hwnd || crop.Empty())
    {
        return false;
    }

    m_instance = instance;
    m_owner = owner;
    m_base = std::move(crop);
    m_freeze = std::move(freeze);
    if (m_freeze.Empty())
    {
        // Still allow annotate; crop refine (grow) needs freeze.
        m_freeze = m_base;
    }

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

    const POINT screen = ImageFToScreen(m_base.screenRect, imagePt);

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
        const DrawStyle style = m_ann.Style(ToolKind::Text);
        if (idx >= 0)
        {
            m_ann.UpdateText(idx, text, &style);
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
    case WM_SETCURSOR:
        if (LOWORD(lParam) == HTCLIENT)
        {
            POINT cursor{};
            GetCursorPos(&cursor);
            ScreenToClient(hwnd, &cursor);
            const CropHit ch = HitCropHandle(cursor);
            if (ch != CropHit::None)
            {
                UpdateCropCursor(ch);
                return TRUE;
            }
        }
        break;

    case WM_LBUTTONDOWN:
    {
        const POINT client{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        const D2D1_POINT_2F pt = D2D1::Point2F(
            static_cast<float>(client.x),
            static_cast<float>(client.y));

        // Edge handles resize the crop against the retained freeze.
        if (m_textEditor && m_textEditor->IsOpen())
        {
            m_textEditor->Close(true);
            return 0;
        }
        const CropHit cropHit = HitCropHandle(client);
        if (cropHit != CropHit::None)
        {
            SetCapture(hwnd);
            m_cropDrag = cropHit;
            m_cropDragStart = { m_base.screenRect.left + client.x, m_base.screenRect.top + client.y };
            m_cropDragStartScreen = m_base.screenRect;
            m_cropPreviewScreen = m_base.screenRect;
            UpdateCropCursor(cropHit);
            return 0;
        }

        if (m_ann.Tool() == ToolKind::Text)
        {
            // PinShot: click existing text → re-edit; blank → place new.
            const int hit = m_ann.HitTest(pt);
            if (hit >= 0)
            {
                if (const auto* sh = m_ann.ShapeAt(hit); sh && sh->kind == ToolKind::Text)
                {
                    OpenTextEditorAt(sh->pts[0], hit);
                    return 0;
                }
            }
            OpenTextEditorAt(pt, -1);
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
    {
        const POINT client{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        if (m_cropDrag != CropHit::None && GetCapture() == hwnd)
        {
            ApplyCropDrag(client);
            return 0;
        }
        if (GetCapture() == hwnd)
        {
            m_ann.Move(D2D1::Point2F(static_cast<float>(client.x), static_cast<float>(client.y)));
        }
        else
        {
            const CropHit ch = HitCropHandle(client);
            if (ch != CropHit::None)
            {
                UpdateCropCursor(ch);
            }
        }
        return 0;
    }
    case WM_LBUTTONUP:
    {
        const POINT client{ GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam) };
        if (m_cropDrag != CropHit::None)
        {
            if (GetCapture() == hwnd)
            {
                ReleaseCapture();
            }
            ApplyCropDrag(client);
            const RECT commit = m_cropPreviewScreen;
            m_cropDrag = CropHit::None;
            m_cropPreviewScreen = {};
            CommitCropRect(commit);
            return 0;
        }
        if (GetCapture() == hwnd)
        {
            ReleaseCapture();
        }
        m_ann.Release(D2D1::Point2F(static_cast<float>(client.x), static_cast<float>(client.y)));
        return 0;
    }

    case WM_LBUTTONDBLCLK:
    {
        // Snipaste/PinShot: double-click text to re-edit (even if not on text tool).
        const D2D1_POINT_2F pt = D2D1::Point2F(
            static_cast<float>(GET_X_LPARAM(lParam)),
            static_cast<float>(GET_Y_LPARAM(lParam)));
        const int hit = m_ann.HitTest(pt);
        if (hit >= 0)
        {
            if (const auto* sh = m_ann.ShapeAt(hit); sh && sh->kind == ToolKind::Text)
            {
                m_ann.SetTool(ToolKind::Text);
                if (m_toolbar)
                {
                    m_toolbar->SyncFromAnnotator();
                }
                OpenTextEditorAt(sh->pts[0], hit);
                return 0;
            }
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
        if (wParam == VK_DELETE || wParam == VK_BACK)
        {
            if (m_ann.DeleteSelected())
            {
                if (m_toolbar)
                {
                    m_toolbar->SyncFromAnnotator();
                }
            }
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
            case 'S':
                setTool(ToolKind::Select);
                return 0;
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
            case 'K':
                setTool(ToolKind::Eraser);
                return 0;
            case 'F':
                m_ann.ToggleFilled();
                if (m_toolbar)
                {
                    m_toolbar->SyncFromAnnotator();
                }
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
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
        }
        break;
    case ToolbarAction::SetColor:
        // color index delivered via Sync / handled in Toolbar click below — see WndProc path
        break;
    case ToolbarAction::ToggleFill:
        m_ann.ToggleFilled();
        if (m_toolbar)
        {
            m_toolbar->SyncFromAnnotator();
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
    // Export must match final image: no select outline, no live preview chrome.
    return m_renderer.Compose(m_base, m_ann, /*includeSelectionChrome=*/false, /*includeLive=*/false);
}

void AnnotationOverlay::DoCopy()
{
    auto composed = ComposeExport();
    if (composed.Empty())
    {
        MessageBoxW(m_hwnd, L"导出合成失败，请重试。", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    if (ImageUtil::CopyToClipboard(composed))
    {
        s_lastExport = std::move(composed);
        s_lastOrigin = ImageOrigin(m_base.screenRect);
        RequestClose();
    }
    else
    {
        MessageBoxW(m_hwnd, L"复制到剪贴板失败，批注仍可继续编辑。", L"SnipClone", MB_ICONWARNING | MB_OK);
    }
}

void AnnotationOverlay::DoSave()
{
    const auto settings = AppSettings::Load();
    auto path = ImageUtil::BrowseSavePng(m_hwnd, settings.outputDir);
    if (path.empty())
    {
        return;
    }
    auto composed = ComposeExport();
    if (composed.Empty())
    {
        MessageBoxW(m_hwnd, L"导出合成失败，请重试。", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    if (ImageUtil::SavePng(composed, path))
    {
        // Remember folder for next save.
        AppSettings updated = settings;
        const auto slash = path.find_last_of(L"\\/");
        if (slash != std::wstring::npos)
        {
            updated.outputDir = path.substr(0, slash);
            updated.Save();
        }
        s_lastExport = std::move(composed);
        s_lastOrigin = ImageOrigin(m_base.screenRect);
        RequestClose();
    }
    else
    {
        MessageBoxW(m_hwnd, L"保存 PNG 失败，批注仍可继续编辑。", L"SnipClone", MB_ICONWARNING | MB_OK);
    }
}

void AnnotationOverlay::DoPin()
{
    auto composed = ComposeExport();
    if (composed.Empty())
    {
        MessageBoxW(m_hwnd, L"导出合成失败，请重试。", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    s_lastOrigin = ImageOrigin(m_base.screenRect);
    if (PinManager::Instance().CreatePin(m_instance, composed, s_lastOrigin) == nullptr)
    {
        MessageBoxW(m_hwnd, L"创建贴图失败，批注仍可继续编辑。", L"SnipClone", MB_ICONWARNING | MB_OK);
        return;
    }
    s_lastExport = std::move(composed);
    RequestClose();
}

AnnotationOverlay::CropHit AnnotationOverlay::HitCropHandle(POINT client) const
{
    if (m_w < 16 || m_h < 16)
    {
        return CropHit::None;
    }
    constexpr int kTol = 7;
    const bool nearL = client.x >= 0 && client.x <= kTol;
    const bool nearR = client.x >= m_w - kTol && client.x < m_w;
    const bool nearT = client.y >= 0 && client.y <= kTol;
    const bool nearB = client.y >= m_h - kTol && client.y < m_h;
    if (nearT && nearL)
        return CropHit::NW;
    if (nearT && nearR)
        return CropHit::NE;
    if (nearB && nearL)
        return CropHit::SW;
    if (nearB && nearR)
        return CropHit::SE;
    if (nearT)
        return CropHit::N;
    if (nearB)
        return CropHit::S;
    if (nearL)
        return CropHit::W;
    if (nearR)
        return CropHit::E;
    return CropHit::None;
}

void AnnotationOverlay::UpdateCropCursor(CropHit hit)
{
    LPCWSTR id = IDC_ARROW;
    switch (hit)
    {
    case CropHit::N:
    case CropHit::S:
        id = IDC_SIZENS;
        break;
    case CropHit::E:
    case CropHit::W:
        id = IDC_SIZEWE;
        break;
    case CropHit::NE:
    case CropHit::SW:
        id = IDC_SIZENESW;
        break;
    case CropHit::NW:
    case CropHit::SE:
        id = IDC_SIZENWSE;
        break;
    default:
        break;
    }
    SetCursor(LoadCursorW(nullptr, id));
}

bool AnnotationOverlay::CommitCropRect(RECT newScreenRect)
{
    if (m_freeze.Empty())
    {
        return false;
    }
    // Clamp to freeze bounds.
    newScreenRect.left = (std::max)(newScreenRect.left, m_freeze.screenRect.left);
    newScreenRect.top = (std::max)(newScreenRect.top, m_freeze.screenRect.top);
    newScreenRect.right = (std::min)(newScreenRect.right, m_freeze.screenRect.right);
    newScreenRect.bottom = (std::min)(newScreenRect.bottom, m_freeze.screenRect.bottom);
    if (newScreenRect.right - newScreenRect.left < 8 || newScreenRect.bottom - newScreenRect.top < 8)
    {
        return false;
    }

    auto cropped = m_freeze.CropToScreenRect(newScreenRect);
    if (cropped.Empty())
    {
        return false;
    }
    if (cropped.screenRect.left == m_base.screenRect.left &&
        cropped.screenRect.top == m_base.screenRect.top &&
        cropped.screenRect.right == m_base.screenRect.right &&
        cropped.screenRect.bottom == m_base.screenRect.bottom)
    {
        Present();
        return true;
    }

    const float dx = static_cast<float>(cropped.screenRect.left - m_base.screenRect.left);
    const float dy = static_cast<float>(cropped.screenRect.top - m_base.screenRect.top);
    // Shapes are crop-local: shift when origin moves, then drop fully outside.
    // Policy: fully outside → removed; partially outside → kept (may clip at edges).
    m_ann.Finish();
    m_ann.RemapForCrop(
        -dx,
        -dy,
        0.f,
        0.f,
        static_cast<float>(cropped.width),
        static_cast<float>(cropped.height));

    m_base = std::move(cropped);
    ReleaseSurface();
    if (!EnsureSurface())
    {
        return false;
    }
    if (m_toolbar)
    {
        m_toolbar->PlaceBelow(m_base.screenRect);
        m_toolbar->SyncFromAnnotator();
    }
    Present();
    return true;
}

void AnnotationOverlay::ApplyCropDrag(POINT client)
{
    // Preview only while dragging; CommitCropRect runs on mouse-up (one undo).
    RECT r = m_cropDragStartScreen;
    const LONG ddx = (m_base.screenRect.left + client.x) - m_cropDragStart.x;
    const LONG ddy = (m_base.screenRect.top + client.y) - m_cropDragStart.y;

    switch (m_cropDrag)
    {
    case CropHit::N:
        r.top += ddy;
        break;
    case CropHit::S:
        r.bottom += ddy;
        break;
    case CropHit::W:
        r.left += ddx;
        break;
    case CropHit::E:
        r.right += ddx;
        break;
    case CropHit::NW:
        r.left += ddx;
        r.top += ddy;
        break;
    case CropHit::NE:
        r.right += ddx;
        r.top += ddy;
        break;
    case CropHit::SW:
        r.left += ddx;
        r.bottom += ddy;
        break;
    case CropHit::SE:
        r.right += ddx;
        r.bottom += ddy;
        break;
    default:
        return;
    }
    if (r.right < r.left)
    {
        std::swap(r.left, r.right);
    }
    if (r.bottom < r.top)
    {
        std::swap(r.top, r.bottom);
    }
    if (r.left < m_freeze.screenRect.left)
        r.left = m_freeze.screenRect.left;
    if (r.top < m_freeze.screenRect.top)
        r.top = m_freeze.screenRect.top;
    if (r.right > m_freeze.screenRect.right)
        r.right = m_freeze.screenRect.right;
    if (r.bottom > m_freeze.screenRect.bottom)
        r.bottom = m_freeze.screenRect.bottom;
    m_cropPreviewScreen = r;
    Present();
}

void AnnotationOverlay::PaintCropChrome()
{
    if (!m_memDc || m_w <= 0 || m_h <= 0)
    {
        return;
    }
    // Display-only chrome (not in ComposeExport).
    int l = 0;
    int t = 0;
    int r = m_w;
    int b = m_h;
    if (m_cropDrag != CropHit::None)
    {
        l = m_cropPreviewScreen.left - m_base.screenRect.left;
        t = m_cropPreviewScreen.top - m_base.screenRect.top;
        r = m_cropPreviewScreen.right - m_base.screenRect.left;
        b = m_cropPreviewScreen.bottom - m_base.screenRect.top;
    }

    HPEN pen = CreatePen(PS_SOLID, 1, RGB(40, 140, 255));
    HGDIOBJ oldPen = SelectObject(m_memDc, pen);
    HGDIOBJ oldBrush = SelectObject(m_memDc, GetStockObject(NULL_BRUSH));
    Rectangle(m_memDc, l, t, r, b);

    HBRUSH fill = CreateSolidBrush(RGB(40, 140, 255));
    SelectObject(m_memDc, fill);
    constexpr int hs = 6;
    const POINT corners[] = {
        { l, t },
        { (l + r) / 2, t },
        { r - 1, t },
        { l, (t + b) / 2 },
        { r - 1, (t + b) / 2 },
        { l, b - 1 },
        { (l + r) / 2, b - 1 },
        { r - 1, b - 1 },
    };
    for (const auto& c : corners)
    {
        RECT rc{ c.x - hs / 2, c.y - hs / 2, c.x + hs / 2 + 1, c.y + hs / 2 + 1 };
        FillRect(m_memDc, &rc, fill);
    }
    SelectObject(m_memDc, oldBrush);
    SelectObject(m_memDc, oldPen);
    DeleteObject(fill);
    DeleteObject(pen);
}

void AnnotationOverlay::StampCaptureToDib(const CaptureResult& image)
{
    if (!m_bits || image.Empty() || m_w <= 0 || m_h <= 0 || m_dibStride <= 0)
    {
        return;
    }
    const int copyW = m_w < image.width ? m_w : image.width;
    const int copyH = m_h < image.height ? m_h : image.height;
    auto* dstBase = static_cast<std::uint8_t*>(m_bits);
    for (int y = 0; y < copyH; ++y)
    {
        const auto* src = image.bgra.data() + static_cast<size_t>(y) * image.stride;
        auto* dst = dstBase + static_cast<size_t>(y) * m_dibStride;
        for (int x = 0; x < copyW; ++x)
        {
            dst[x * 4 + 0] = src[x * 4 + 0];
            dst[x * 4 + 1] = src[x * 4 + 1];
            dst[x * 4 + 2] = src[x * 4 + 2];
            dst[x * 4 + 3] = 255; // ULW_ALPHA requires opaque for solid photo
        }
    }
}

bool AnnotationOverlay::EnsureSurface()
{
    const int w = m_base.width;
    const int h = m_base.height;
    if (w <= 0 || h <= 0 || m_base.Empty())
    {
        return false;
    }
    if (m_w == w && m_h == h && m_bits && m_memDc && m_dib)
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
    if (!m_dib || !m_bits)
    {
        return false;
    }
    m_oldBmp = SelectObject(m_memDc, m_dib);

    BITMAP bm{};
    if (GetObjectW(m_dib, sizeof(bm), &bm) && bm.bmWidthBytes > 0)
    {
        m_dibStride = bm.bmWidthBytes;
    }
    else
    {
        m_dibStride = w * 4;
    }

    m_w = w;
    m_h = h;

    // Seed with freeze crop so the host is never blank before first Compose.
    StampCaptureToDib(m_base);
    return m_bits != nullptr;
}

void AnnotationOverlay::ReleaseSurface()
{
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
    m_dibStride = 0;
    m_w = m_h = 0;
}

void AnnotationOverlay::Present()
{
    if (!m_hwnd || !EnsureSurface())
    {
        return;
    }

    // Preview Compose keeps live stroke + selection chrome; export strips both.
    CaptureResult frame = m_renderer.Compose(m_base, m_ann, true, true);
    if (frame.Empty())
    {
        StampCaptureToDib(m_base);
    }
    else
    {
        StampCaptureToDib(frame);
    }
    PaintCropChrome(); // handles are display-only; ComposeExport never paints them

    // Keep window size in sync with capture pixels (DPI-safe).
    SetWindowPos(
        m_hwnd,
        HWND_TOPMOST,
        m_base.screenRect.left,
        m_base.screenRect.top,
        m_w,
        m_h,
        SWP_NOACTIVATE);

    POINT ptSrc{ 0, 0 };
    SIZE size{ m_w, m_h };
    POINT ptDst{ m_base.screenRect.left, m_base.screenRect.top };
    BLENDFUNCTION blend{ AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
    UpdateLayeredWindow(m_hwnd, m_screenDc, &ptDst, &size, m_memDc, &ptSrc, 0, &blend, ULW_ALPHA);
}

} // namespace snip
