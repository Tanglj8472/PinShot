//==============================================================================
// SnipClone — global hotkey wrapper (RegisterHotKey)
//==============================================================================
#pragma once

#include <Windows.h>

namespace snip
{

class GlobalHotkey
{
public:
    static bool Register(UINT vk, UINT modifiers, int id, HWND hwnd)
    {
        return RegisterHotKey(hwnd, id, modifiers, vk) != FALSE;
    }

    static bool Unregister(int id, HWND hwnd)
    {
        return UnregisterHotKey(hwnd, id) != FALSE;
    }
};

} // namespace snip
