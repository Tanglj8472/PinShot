//==============================================================================
// SnipClone — tray icon
//==============================================================================
#include "TrayIcon.h"

#include "../PinWindow/PinManager.h"
#include "AppSettings.h"
#include "ProjectIO.h"

#include <shellapi.h>

#pragma comment(lib, "shell32.lib")

namespace snip
{

TrayIcon::~TrayIcon()
{
    Destroy();
}

bool TrayIcon::Create(HWND owner, HINSTANCE instance, const std::wstring& tip, CommandFn onCommand)
{
    if (m_added || !owner)
    {
        return false;
    }
    m_owner = owner;
    m_instance = instance;
    m_onCommand = std::move(onCommand);
    m_tip = tip;

    ZeroMemory(&m_nid, sizeof(m_nid));
    m_nid.cbSize = sizeof(m_nid);
    m_nid.hWnd = owner;
    m_nid.uID = 1;
    m_nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    m_nid.uCallbackMessage = WM_TRAY;
    m_nid.hIcon = LoadIconW(nullptr, IDI_APPLICATION);
    wcsncpy_s(m_nid.szTip, m_tip.c_str(), _TRUNCATE);

    if (!Shell_NotifyIconW(NIM_ADD, &m_nid))
    {
        return false;
    }
    m_added = true;

    m_nid.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &m_nid);
    return true;
}

bool TrayIcon::Recreate()
{
    if (!m_owner)
    {
        return false;
    }
    if (m_added)
    {
        Shell_NotifyIconW(NIM_DELETE, &m_nid);
        m_added = false;
    }
    m_nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    wcsncpy_s(m_nid.szTip, m_tip.c_str(), _TRUNCATE);
    if (!Shell_NotifyIconW(NIM_ADD, &m_nid))
    {
        return false;
    }
    m_added = true;
    m_nid.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &m_nid);
    return true;
}

void TrayIcon::Destroy()
{
    if (m_added)
    {
        Shell_NotifyIconW(NIM_DELETE, &m_nid);
        m_added = false;
    }
}

void TrayIcon::SetClickThroughHotkeyTip(const std::wstring& tip)
{
    m_clickThroughTip = tip.empty() ? L"Ctrl+Shift+P" : tip;
}

void TrayIcon::ShowBalloon(const wchar_t* title, const wchar_t* body)
{
    if (!m_added)
    {
        return;
    }
    m_nid.uFlags = NIF_INFO;
    wcsncpy_s(m_nid.szInfoTitle, title ? title : L"SnipClone", _TRUNCATE);
    wcsncpy_s(m_nid.szInfo, body ? body : L"", _TRUNCATE);
    m_nid.dwInfoFlags = NIIF_INFO;
    Shell_NotifyIconW(NIM_MODIFY, &m_nid);
    m_nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
}

void TrayIcon::PopupMenu()
{
    POINT pt{};
    GetCursorPos(&pt);
    HMENU menu = CreatePopupMenu();
    if (!menu)
    {
        return;
    }
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::Snip), L"截图");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::ShowPins), L"显示全部贴图");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::HidePins), L"隐藏全部贴图");
    {
        std::wstring exitThrough = L"退出贴图穿透（";
        exitThrough += m_clickThroughTip;
        exitThrough += L"）";
        AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::ClearClickThrough), exitThrough.c_str());
    }
    AppendMenuW(
        menu,
        MF_STRING | (PinManager::Instance().HasLastClosed() ? 0 : MF_GRAYED),
        static_cast<UINT>(Command::RestoreLastPin),
        L"恢复最近关闭的贴图");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::Settings), L"设置…");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::OpenProject), L"打开项目…");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::OpenImage), L"打开图片标注…");
    AppendMenuW(
        menu,
        MF_STRING | (ProjectIO::HasAutosave() ? 0 : MF_GRAYED),
        static_cast<UINT>(Command::RestoreAutosave),
        L"恢复自动保存的项目");
    AppendMenuW(
        menu,
        MF_STRING | (ProjectIO::HasAutosave() ? 0 : MF_GRAYED),
        static_cast<UINT>(Command::ClearAutosave),
        L"清除自动保存");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::OpenOutputDir), L"打开输出目录");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::OpenSettingsDir), L"打开设置目录（settings.ini）");
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::About), L"关于");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, static_cast<UINT>(Command::Exit), L"退出");

    SetForegroundWindow(m_owner);
    const UINT cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        m_owner,
        nullptr);
    DestroyMenu(menu);
    PostMessageW(m_owner, WM_NULL, 0, 0);
    if (cmd && m_onCommand)
    {
        m_onCommand(static_cast<Command>(cmd));
    }
}

bool TrayIcon::HandleMessage(UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (msg != WM_TRAY)
    {
        return false;
    }
    const UINT notify = static_cast<UINT>(LOWORD(lParam));
    if (notify == WM_RBUTTONUP || notify == WM_CONTEXTMENU)
    {
        PopupMenu();
        return true;
    }
    // M1 / Snipaste-like: single click starts snip (dblclick kept as alias).
    if (notify == WM_LBUTTONUP || notify == WM_LBUTTONDBLCLK)
    {
        if (m_onCommand)
        {
            m_onCommand(Command::Snip);
        }
        return true;
    }
    return true;
}

} // namespace snip
