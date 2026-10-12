//==============================================================================
// SnipClone — floating text editor
//==============================================================================
#include "TextEditor.h"

#include "../Common/WinHelper.h"

#include <commctrl.h>
#include <imm.h>
#include <windowsx.h>

#include <algorithm>
#include <cmath>
#include <string>

#pragma comment(lib, "comctl32.lib")
#pragma comment(lib, "imm32.lib")

namespace snip
{
namespace
{
constexpr wchar_t kHostClass[] = L"SnipCloneTextEditorHost";
constexpr UINT_PTR kSubclassId = 1;
}

TextEditor::~TextEditor()
{
    Destroy();
}

HFONT TextEditor::MakeFont(const DrawStyle& style) const
{
    const int px = (std::max)(8, static_cast<int>(std::lround(style.fontSize)));
    return CreateFontW(
        -px,
        0,
        0,
        0,
        style.bold ? FW_BOLD : FW_NORMAL,
        style.italic ? TRUE : FALSE,
        FALSE,
        FALSE,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_SWISS,
        L"Segoe UI");
}

bool TextEditor::Open(
    HINSTANCE instance,
    HWND owner,
    POINT screenPos,
    const DrawStyle& style,
    const std::wstring& initial,
    DoneFn onDone)
{
    if (IsOpen())
    {
        Close(true);
    }

    m_instance = instance;
    m_owner = owner;
    m_style = style;
    m_onDone = std::move(onDone);
    m_closing = false;

    INITCOMMONCONTROLSEX icc{ sizeof(icc), ICC_STANDARD_CLASSES };
    InitCommonControlsEx(&icc);

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = HostProc;
    wc.hInstance = instance;
    wc.hCursor = LoadCursorW(nullptr, IDC_IBEAM);
    wc.hbrBackground = reinterpret_cast<HBRUSH>(GetStockObject(WHITE_BRUSH));
    wc.lpszClassName = kHostClass;
    EnsureWindowClass(wc);

    const int h = (std::max)(24, static_cast<int>(style.fontSize) + 8);
    const int w = 120;

    m_host = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        kHostClass,
        L"",
        WS_POPUP | WS_VISIBLE,
        screenPos.x,
        screenPos.y,
        w,
        h,
        owner,
        nullptr,
        instance,
        this);
    if (!m_host)
    {
        return false;
    }

    m_edit = CreateWindowExW(
        0,
        L"EDIT",
        initial.c_str(),
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | ES_LEFT,
        2,
        2,
        w - 4,
        h - 4,
        m_host,
        nullptr,
        instance,
        nullptr);
    if (!m_edit)
    {
        Destroy();
        return false;
    }

    SetWindowSubclass(m_edit, EditSubclass, kSubclassId, reinterpret_cast<DWORD_PTR>(this));
    ApplyStyle(style);
    SetFocus(m_edit);
    SendMessageW(m_edit, EM_SETSEL, 0, -1);
    ResizeToContent();
    return true;
}

void TextEditor::ApplyStyle(const DrawStyle& style)
{
    m_style = style;
    if (!m_edit)
    {
        return;
    }
    if (m_font)
    {
        DeleteObject(m_font);
        m_font = nullptr;
    }
    m_font = MakeFont(style);
    SendMessageW(m_edit, WM_SETFONT, reinterpret_cast<WPARAM>(m_font), TRUE);

    // Text color via EM — EDIT uses system colors; approximate via parent invalidate.
    InvalidateRect(m_host, nullptr, TRUE);
    ResizeToContent();
}

void TextEditor::ResizeToContent()
{
    if (!m_host || !m_edit)
    {
        return;
    }

    wchar_t buf[1024]{};
    GetWindowTextW(m_edit, buf, 1024);
    std::wstring measure = buf;
    measure += L"MM";

    HDC hdc = GetDC(m_edit);
    HFONT old = m_font ? static_cast<HFONT>(SelectObject(hdc, m_font)) : nullptr;
    SIZE sz{};
    GetTextExtentPoint32W(hdc, measure.c_str(), static_cast<int>(measure.size()), &sz);
    if (old)
    {
        SelectObject(hdc, old);
    }
    ReleaseDC(m_edit, hdc);

    const int h = (std::max)(
        static_cast<int>(sz.cy) + 8,
        static_cast<int>(m_style.fontSize) + 8);
    int w = (std::max)(80, static_cast<int>(sz.cx) + 16);

    // A10: clamp editor into the monitor work area; keep top-left anchored.
    RECT wr{};
    GetWindowRect(m_host, &wr);
    RECT work{};
    MONITORINFO mi{ sizeof(mi) };
    HMONITOR mon = MonitorFromWindow(m_host, MONITOR_DEFAULTTONEAREST);
    if (mon && GetMonitorInfoW(mon, &mi))
    {
        work = mi.rcWork;
    }
    else
    {
        work = { 0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN) };
    }
    const int maxW = (std::max)(80, static_cast<int>(work.right - wr.left - 8));
    const int maxH = (std::max)(24, static_cast<int>(work.bottom - wr.top - 8));
    w = (std::min)(w, maxW);
    const int hh = (std::min)(h, maxH);
    int x = wr.left;
    int y = wr.top;
    if (x + w > work.right)
    {
        x = work.right - w;
    }
    if (y + hh > work.bottom)
    {
        y = work.bottom - hh;
    }
    if (x < work.left)
    {
        x = work.left;
    }
    if (y < work.top)
    {
        y = work.top;
    }

    SetWindowPos(m_host, HWND_TOPMOST, x, y, w, hh, SWP_NOACTIVATE);
    MoveWindow(m_edit, 2, 2, w - 4, hh - 4, TRUE);
}

bool TextEditor::IsImeComposing() const
{
    if (!m_edit)
    {
        return false;
    }
    HIMC imc = ImmGetContext(m_edit);
    if (!imc)
    {
        return false;
    }
    const LONG bytes = ImmGetCompositionStringW(imc, GCS_COMPSTR, nullptr, 0);
    ImmReleaseContext(m_edit, imc);
    return bytes > 0;
}

void TextEditor::ScheduleBlurCommit()
{
    if (m_host && !m_closing)
    {
        PostMessageW(m_host, WM_TE_BLUR_COMMIT, 0, 0);
    }
}

void TextEditor::Close(bool commit)
{
    if (m_closing || !m_host)
    {
        return;
    }
    m_closing = true;

    std::wstring text;
    if (m_edit)
    {
        const int len = GetWindowTextLengthW(m_edit);
        text.resize(static_cast<size_t>(len) + 1);
        GetWindowTextW(m_edit, text.data(), len + 1);
        text.resize(static_cast<size_t>(len));
    }

    // Trim
    while (!text.empty() && (text.back() == L' ' || text.back() == L'\t'))
    {
        text.pop_back();
    }
    size_t start = 0;
    while (start < text.size() && (text[start] == L' ' || text[start] == L'\t'))
    {
        ++start;
    }
    text = text.substr(start);

    auto cb = std::move(m_onDone);
    m_onDone = nullptr;

    Destroy();

    if (cb)
    {
        cb(commit && !text.empty(), text);
    }
}

void TextEditor::Destroy()
{
    if (m_edit)
    {
        RemoveWindowSubclass(m_edit, EditSubclass, kSubclassId);
        DestroyWindow(m_edit);
        m_edit = nullptr;
    }
    if (m_host)
    {
        DestroyWindow(m_host);
        m_host = nullptr;
    }
    if (m_font)
    {
        DeleteObject(m_font);
        m_font = nullptr;
    }
    m_closing = false;
}

LRESULT CALLBACK TextEditor::HostProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    TextEditor* self = reinterpret_cast<TextEditor*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if (msg == WM_NCCREATE)
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        self = static_cast<TextEditor*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(self));
        return TRUE;
    }

    switch (msg)
    {
    case WM_CTLCOLOREDIT:
        if (self)
        {
            HDC hdc = reinterpret_cast<HDC>(wParam);
            const auto& c = self->m_style.color;
            SetTextColor(hdc, RGB(
                static_cast<BYTE>(c.r * 255.f),
                static_cast<BYTE>(c.g * 255.f),
                static_cast<BYTE>(c.b * 255.f)));
            SetBkColor(hdc, RGB(255, 255, 255));
            static HBRUSH brush = CreateSolidBrush(RGB(255, 255, 255));
            return reinterpret_cast<LRESULT>(brush);
        }
        break;
    case WM_COMMAND:
        if (self && HIWORD(wParam) == EN_CHANGE)
        {
            self->ResizeToContent();
        }
        return 0;
    case WM_TE_BLUR_COMMIT:
        if (self && !self->m_closing)
        {
            HWND focus = GetFocus();
            if (focus && (focus == self->m_edit || focus == self->m_host || IsChild(self->m_host, focus)))
            {
                return 0;
            }
            // Toolbar/StylePanel use WS_EX_NOACTIVATE — focus rarely moves there.
            // If focus left the editor (canvas / other app), commit like PinShot blur.
            if (self->IsImeComposing())
            {
                return 0;
            }
            self->Close(true);
        }
        return 0;
    case WM_PAINT:
    {
        PAINTSTRUCT ps{};
        HDC hdc = BeginPaint(hwnd, &ps);
        RECT rc{};
        GetClientRect(hwnd, &rc);
        // Semi-transparent look approximated: white fill + dashed color border (PinShot-like).
        FillRect(hdc, &rc, static_cast<HBRUSH>(GetStockObject(WHITE_BRUSH)));
        if (self)
        {
            const auto& c = self->m_style.color;
            HPEN pen = CreatePen(
                PS_DASH,
                1,
                RGB(static_cast<BYTE>(c.r * 255.f), static_cast<BYTE>(c.g * 255.f), static_cast<BYTE>(c.b * 255.f)));
            HGDIOBJ old = SelectObject(hdc, pen);
            SelectObject(hdc, GetStockObject(NULL_BRUSH));
            Rectangle(hdc, rc.left, rc.top, rc.right, rc.bottom);
            SelectObject(hdc, old);
            DeleteObject(pen);
        }
        EndPaint(hwnd, &ps);
        return 0;
    }
    case WM_DESTROY:
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

LRESULT CALLBACK TextEditor::EditSubclass(
    HWND hwnd,
    UINT msg,
    WPARAM wParam,
    LPARAM lParam,
    UINT_PTR /*id*/,
    DWORD_PTR data)
{
    auto* self = reinterpret_cast<TextEditor*>(data);
    if (msg == WM_KEYDOWN)
    {
        if (wParam == VK_RETURN)
        {
            // Let IME confirm candidate / finish composition first — do not submit snip text.
            if (self->IsImeComposing())
            {
                return DefSubclassProc(hwnd, msg, wParam, lParam);
            }
            self->Close(true);
            return 0;
        }
        if (wParam == VK_ESCAPE)
        {
            if (self->IsImeComposing())
            {
                return DefSubclassProc(hwnd, msg, wParam, lParam);
            }
            self->Close(false);
            return 0;
        }
    }
    if (msg == WM_KILLFOCUS)
    {
        self->ScheduleBlurCommit();
    }
    return DefSubclassProc(hwnd, msg, wParam, lParam);
}

} // namespace snip
