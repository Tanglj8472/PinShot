//==============================================================================
// SnipClone — shared DirectWrite text extent (hit / chrome / draw / editor)
//==============================================================================
#pragma once

#include "../AnnotationOverlay/DrawCommand.h"

#include <string>

namespace snip
{

struct TextExtent
{
    float width = 0.f;
    float height = 0.f;
};

// Same metrics for ApproxBounds, selection chrome, and draw layout width.
TextExtent MeasureAnnotationText(const std::wstring& text, const DrawStyle& style);

} // namespace snip
