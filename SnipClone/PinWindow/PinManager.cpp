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
    m_hidden = false;
    return pin;
}

void PinManager::RequestClose(PinWindow* pin)
{
    if (!pin)
    {
        return;
    }
    m_lastClosed = pin->Snapshot();
    if (pin->Window())
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
        PinWindow* pin = m_pins.front();
        m_lastClosed = pin->Snapshot();
        HWND hwnd = pin->Window();
        if (hwnd)
        {
            DestroyWindow(hwnd);
        }
        else
        {
            m_pins.erase(m_pins.begin());
            delete pin;
        }
    }
    m_hidden = false;
}

void PinManager::HideAll()
{
    for (auto* pin : m_pins)
    {
        pin->SetVisible(false);
    }
    m_hidden = true;
}

void PinManager::ShowAll()
{
    for (auto* pin : m_pins)
    {
        pin->SetVisible(true);
        if (pin->ClickThrough())
        {
            // Keep click-through; just ensure topmost refresh.
            pin->SetClickThrough(true);
        }
    }
    m_hidden = false;
}

void PinManager::ClearClickThroughAll()
{
    for (auto* pin : m_pins)
    {
        pin->SetClickThrough(false);
    }
    ShowAll();
}

bool PinManager::RestoreLastClosed(HINSTANCE instance)
{
    if (m_lastClosed.image.Empty())
    {
        return false;
    }
    auto* pin = new PinWindow();
    PinSnapshot snap = m_lastClosed;
    if (!pin->CreateFromSnapshot(instance, std::move(snap)))
    {
        delete pin;
        return false;
    }
    m_pins.push_back(pin);
    m_hidden = false;
    return true;
}

} // namespace snip
