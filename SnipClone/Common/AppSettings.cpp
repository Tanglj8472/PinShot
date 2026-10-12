//==============================================================================
// SnipClone — settings.ini under %APPDATA%\SnipClone
//==============================================================================
#include "AppSettings.h"

#include <shlobj.h>

#include <cstdio>

namespace snip
{
namespace
{
std::wstring JoinPath(const std::wstring& a, const std::wstring& b)
{
    if (a.empty())
    {
        return b;
    }
    if (a.back() == L'\\' || a.back() == L'/')
    {
        return a + b;
    }
    return a + L'\\' + b;
}

UINT ReadUint(const wchar_t* section, const wchar_t* key, UINT def, const wchar_t* path)
{
    return static_cast<UINT>(GetPrivateProfileIntW(section, key, static_cast<INT>(def), path));
}

void WriteUint(const wchar_t* section, const wchar_t* key, UINT value, const wchar_t* path)
{
    wchar_t buf[32]{};
    swprintf_s(buf, L"%u", value);
    WritePrivateProfileStringW(section, key, buf, path);
}
} // namespace

std::wstring AppSettings::SettingsDir()
{
    wchar_t appdata[MAX_PATH]{};
    if (FAILED(SHGetFolderPathW(nullptr, CSIDL_APPDATA, nullptr, SHGFP_TYPE_CURRENT, appdata)))
    {
        return {};
    }
    return JoinPath(appdata, L"SnipClone");
}

std::wstring AppSettings::SettingsPath()
{
    const auto dir = SettingsDir();
    if (dir.empty())
    {
        return {};
    }
    return JoinPath(dir, L"settings.ini");
}

AppSettings AppSettings::Load()
{
    AppSettings s;
    const auto path = SettingsPath();
    if (path.empty())
    {
        return s;
    }
    // Defaults already set in struct; override from ini when present.
    s.snip.vk = ReadUint(L"Hotkey", L"SnipVk", s.snip.vk, path.c_str());
    s.snip.mods = ReadUint(L"Hotkey", L"SnipMods", s.snip.mods, path.c_str());
    s.pins.vk = ReadUint(L"Hotkey", L"PinsVk", s.pins.vk, path.c_str());
    s.pins.mods = ReadUint(L"Hotkey", L"PinsMods", s.pins.mods, path.c_str());
    s.startWithWindows = ReadUint(L"General", L"StartWithWindows", 0, path.c_str()) != 0;

    wchar_t outDir[MAX_PATH]{};
    GetPrivateProfileStringW(L"General", L"OutputDir", L"", outDir, MAX_PATH, path.c_str());
    s.outputDir = outDir;
    return s;
}

bool AppSettings::Save() const
{
    const auto dir = SettingsDir();
    if (dir.empty())
    {
        return false;
    }
    CreateDirectoryW(dir.c_str(), nullptr);
    const auto path = SettingsPath();
    WriteUint(L"Hotkey", L"SnipVk", snip.vk, path.c_str());
    WriteUint(L"Hotkey", L"SnipMods", snip.mods, path.c_str());
    WriteUint(L"Hotkey", L"PinsVk", pins.vk, path.c_str());
    WriteUint(L"Hotkey", L"PinsMods", pins.mods, path.c_str());
    WriteUint(L"General", L"StartWithWindows", startWithWindows ? 1u : 0u, path.c_str());
    WritePrivateProfileStringW(L"General", L"OutputDir", outputDir.c_str(), path.c_str());
    return true;
}

std::wstring AppSettings::FormatHotkey(const HotkeySpec& hk)
{
    std::wstring s;
    if (hk.mods & MOD_CONTROL)
        s += L"Ctrl+";
    if (hk.mods & MOD_SHIFT)
        s += L"Shift+";
    if (hk.mods & MOD_ALT)
        s += L"Alt+";
    if (hk.mods & MOD_WIN)
        s += L"Win+";
    if (hk.vk >= 'A' && hk.vk <= 'Z')
    {
        s += static_cast<wchar_t>(hk.vk);
    }
    else
    {
        wchar_t buf[16]{};
        swprintf_s(buf, L"0x%02X", hk.vk);
        s += buf;
    }
    return s;
}

} // namespace snip
