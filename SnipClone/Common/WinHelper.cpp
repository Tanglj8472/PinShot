//==============================================================================
// SnipClone — window / layered-window helpers
//==============================================================================
#include "WinHelper.h"

#include "DpiHelper.h"

namespace snip
{

bool EnsureWindowClass(const WNDCLASSEXW& wc)
{
    if (RegisterClassExW(&wc) != 0)
    {
        return true;
    }

    if (GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
    {
        return false;
    }

    WNDCLASSEXW existing{};
    existing.cbSize = sizeof(existing);
    if (!GetClassInfoExW(wc.hInstance, wc.lpszClassName, &existing))
    {
        return false;
    }
    return existing.lpfnWndProc == wc.lpfnWndProc;
}

HWND CreateTopmostLayeredPopup(
    HWND owner,
    HINSTANCE instance,
    const wchar_t* className,
    void* createParam)
{
    const RECT screen = GetVirtualScreenRect();
    const DWORD exStyle = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW;
    return CreateWindowExW(
        exStyle,
        className,
        L"",
        WS_POPUP,
        screen.left,
        screen.top,
        screen.right - screen.left,
        screen.bottom - screen.top,
        owner,
        nullptr,
        instance,
        createParam);
}

} // namespace snip
