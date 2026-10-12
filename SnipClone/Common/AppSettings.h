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
    // After committing a drawable annotation, switch to Select so handles are usable.
    bool selectAfterDraw = true;

    static AppSettings Load();
    bool Save() const;

    // Built-in factory defaults (hotkeys + annotation behavior). Does not touch outputDir.
    static AppSettings Defaults();
    void RestoreDefaultHotkeys();

    static std::wstring SettingsDir();
    static std::wstring SettingsPath();
    static std::wstring FormatHotkey(const HotkeySpec& hk);
};

// Process-wide live settings (A04). Load once at startup; windows must mutate this
// instance so exit Save cannot overwrite a fresher disk copy with a stale snapshot.
AppSettings& LiveSettings();
void LoadLiveSettings();

} // namespace snip
