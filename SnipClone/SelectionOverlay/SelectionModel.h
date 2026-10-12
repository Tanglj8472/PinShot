//==============================================================================
// SnipClone — selection rectangle model (screen coordinates)
//==============================================================================
#pragma once

#include <Windows.h>

namespace snip
{

struct SelectionModel
{
    // Absolute screen coordinates on the virtual desktop.
    RECT screenRect{};
    bool hasSelection = false;

    LONG Width() const { return screenRect.right - screenRect.left; }
    LONG Height() const { return screenRect.bottom - screenRect.top; }

    void Clear()
    {
        screenRect = {};
        hasSelection = false;
    }

    void Set(const RECT& rect)
    {
        screenRect = rect;
        hasSelection = (Width() > 0 && Height() > 0);
    }
};

} // namespace snip
