//==============================================================================
// SnipClone — pin instance manager
//==============================================================================
#include "PinManager.h"

#include "PinWindow.h"

#include <algorithm>

namespace snip
{

PinManager& PinManager::Instance()
{
    static PinManager inst;
    return inst;
}

PinWindow* PinManager::CreatePin(HINSTANCE instance, const CaptureResult& image, POINT screenOrigin)
{
    auto* pin = new PinWindow();
    if (!pin->Create(instance, image, screenOrigin))
    {
        delete pin;
        return nullptr;
    }
    m_pins.push_back(pin);
    return pin;
}

void PinManager::RequestClose(PinWindow* pin)
{
    if (pin && pin->Window())
    {
        PostMessageW(pin->Window(), WM_CLOSE, 0, 0);
    }
}

void PinManager::OnNcDestroy(PinWindow* pin)
{
    auto it = std::find(m_pins.begin(), m_pins.end(), pin);
    if (it != m_pins.end())
    {
        m_pins.erase(it);
    }
    delete pin;
}

void PinManager::CloseAll()
{
    while (!m_pins.empty())
    {
        HWND hwnd = m_pins.front()->Window();
        if (hwnd)
        {
            DestroyWindow(hwnd);
        }
        else
        {
            delete m_pins.front();
            m_pins.erase(m_pins.begin());
        }
    }
}

} // namespace snip
