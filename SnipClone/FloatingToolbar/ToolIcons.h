//==============================================================================
// SnipClone — D2D toolbar icons（对齐 PinShot icons.py 自绘）
//==============================================================================
#pragma once

#include "ToolButton.h"

#include <d2d1.h>

namespace snip
{

void DrawToolbarIcon(
    ID2D1RenderTarget* rt,
    ID2D1SolidColorBrush* brush,
    const D2D1_RECT_F& cell,
    ToolbarAction action,
    ToolKind tool);

} // namespace snip
