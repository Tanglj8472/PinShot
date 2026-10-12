//==============================================================================
// SnipClone — editable annotation document (M2 pin re-edit)
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "DrawCommand.h"

#include <vector>

namespace snip
{

// Minimal document: unannotated crop + optional freeze + vector shapes.
// Composed bitmap is a view/cache owned by the pin, not the sole source of truth.
struct AnnotatorDocument
{
    CaptureResult base;   // crop without annotations
    CaptureResult freeze; // full freeze for crop refine; may equal base
    std::vector<DrawCommand> shapes;

    bool Editable() const { return !base.Empty(); }
};

inline constexpr UINT WM_SNIP_PIN_EDIT = WM_APP + 0x53; // lParam = PinWindow*

} // namespace snip
