//==============================================================================
// SnipClone — pin instance manager（对齐 PinShot ALL_PINS 角色）
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"

#include "../Common/WinCompat.h"

#include <vector>

namespace snip
{

class PinWindow;

class PinManager
{
public:
    static PinManager& Instance();

    PinWindow* CreatePin(HINSTANCE instance, const CaptureResult& image, POINT screenOrigin);
    void RequestClose(PinWindow* pin);

    // WM_NCDESTROY: remove + delete (classic Win32 self-delete).
    void OnNcDestroy(PinWindow* pin);

    void CloseAll();

private:
    std::vector<PinWindow*> m_pins;
};

} // namespace snip
