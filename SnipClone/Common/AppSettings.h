//==============================================================================
// SnipClone — persisted settings (%APPDATA%\SnipClone\settings.ini)
//==============================================================================
#pragma once

#include "WinCompat.h"

#include <string>

namespace snip
{

struct HotkeySpec
{
    UINT vk = 0;
    UINT mods = 0; // MOD_CONTROL | MOD_SHIFT | …
};

struct AppSettings
{
    HotkeySpec snip{ static_cast<UINT>('S'), MOD_CONTROL | MOD_SHIFT };
    HotkeySpec pins{ static_cast<UINT>('P'), MOD_CONTROL | MOD_SHIFT };
    std::wstring outputDir; // empty → user Documents or last Browse path
    bool startWithWindows = false;

    static AppSettings Load();
    bool Save() const;

    static std::wstring SettingsDir();
    static std::wstring SettingsPath();
    static std::wstring FormatHotkey(const HotkeySpec& hk);
};

} // namespace snip
