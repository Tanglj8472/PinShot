//==============================================================================
// SnipClone — pin instance manager（对齐 PinShot ALL_PINS 角色）
//==============================================================================
#pragma once

#include "PinWindow.h"

#include "../Capture/CaptureResult.h"

#include "../Common/WinCompat.h"

#include <vector>

namespace snip
{

class PinManager
{
public:
    static PinManager& Instance();

    PinWindow* CreatePin(HINSTANCE instance, const CaptureResult& image, POINT screenOrigin);
    void RequestClose(PinWindow* pin);

    // WM_NCDESTROY: remove + delete (classic Win32 self-delete).
    void OnNcDestroy(PinWindow* pin);

    void CloseAll();
    void HideAll();
    void ShowAll();
    void ClearClickThroughAll();
    bool RestoreLastClosed(HINSTANCE instance);
    bool HasLastClosed() const { return !m_lastClosed.image.Empty(); }
    size_t Count() const { return m_pins.size(); }

private:
    std::vector<PinWindow*> m_pins;
    PinSnapshot m_lastClosed{};
    bool m_hidden = false;
};

} // namespace snip
