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

void PinManager::PushClosed(PinSnapshot snap)
{
    if (snap.image.Empty())
    {
        return;
    }
    m_closedStack.push_back(std::move(snap));
    while (m_closedStack.size() > kMaxClosed)
    {
        m_closedStack.erase(m_closedStack.begin());
    }
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
    // A12: while session is hidden, only the new pin is shown; others stay hidden.
    if (m_hidden)
    {
        pin->SetVisible(true);
    }
    else
    {
        pin->SetVisible(true);
    }
    return pin;
}

void PinManager::RequestClose(PinWindow* pin)
{
    if (!pin)
    {
        return;
    }
    PushClosed(pin->Snapshot());
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
        PushClosed(pin->Snapshot());
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
    // A12: consume one closed snapshot (no duplicate restores of the same pin).
    if (m_closedStack.empty())
    {
        return false;
    }
    // Pop only after successful create — failed restore must not consume history (M1).
    PinSnapshot snap = m_closedStack.back();

    auto* pin = new PinWindow();
    if (!pin->CreateFromSnapshot(instance, std::move(snap)))
    {
        delete pin;
        return false;
    }
    m_closedStack.pop_back();
    m_pins.push_back(pin);
    if (m_hidden)
    {
        // Restored into a hidden session: show only this pin.
        pin->SetVisible(true);
    }
    return true;
}

} // namespace snip
