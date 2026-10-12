//==============================================================================
// SnipClone — minimal settings UI (M1: hotkeys + output dir)
//==============================================================================
#pragma once

#include "AppSettings.h"
#include "WinCompat.h"

namespace snip
{

// Modal settings. Returns true if user applied changes (and Save succeeded).
// onApplied: re-register hotkeys / refresh tray tip after a successful Apply.
bool ShowSettingsDialog(HWND owner, HINSTANCE instance);

// HKCU\Software\Microsoft\Windows\CurrentVersion\Run "SnipClone"
bool ApplyStartWithWindows(bool enable, std::wstring* outError = nullptr);
bool IsStartWithWindowsRegistered();

} // namespace snip
