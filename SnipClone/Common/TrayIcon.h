//==============================================================================
// SnipClone — notification-area icon + context menu
//==============================================================================
#pragma once

#include "WinCompat.h"

#include <shellapi.h>

#include <functional>
#include <string>

namespace snip
{

class TrayIcon
{
public:
    enum class Command
    {
        Snip = 1,
        ShowPins,
        HidePins,
        ClearClickThrough,
        OpenOutputDir,
        OpenSettingsDir,
        About,
        Exit,
    };

    using CommandFn = std::function<void(Command)>;

    TrayIcon() = default;
    ~TrayIcon();

    bool Create(HWND owner, HINSTANCE instance, const std::wstring& tip, CommandFn onCommand);
    void Destroy();
    void ShowBalloon(const wchar_t* title, const wchar_t* body);
    bool HandleMessage(UINT msg, WPARAM wParam, LPARAM lParam);

    static constexpr UINT WM_TRAY = WM_APP + 0x71;

private:
    void PopupMenu();

    HWND m_owner = nullptr;
    HINSTANCE m_instance = nullptr;
    NOTIFYICONDATAW m_nid{};
    bool m_added = false;
    CommandFn m_onCommand;
};

} // namespace snip
