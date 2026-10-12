//==============================================================================
// SnipClone — toolbar button descriptors（对齐 PinShot ToolDock 按钮角色）
//==============================================================================
#pragma once

#include "../AnnotationOverlay/DrawCommand.h"

#include "../Common/WinCompat.h"

#include <string>

namespace snip
{

enum class ToolbarAction
{
    None = 0,
    SetTool,
    Undo,
    Redo,
    WidthDec,
    WidthInc,
    ColorCycle,
    SetColor,
    ToggleFill,
    Bold,
    Italic,
    Outline,
    Close,
    Pin,
    Save,
    Copy,
};

struct ToolButton
{
    ToolbarAction action = ToolbarAction::None;
    ToolKind tool = ToolKind::None; // when action == SetTool
    int colorIndex = -1;            // when action == SetColor
    std::wstring id;                // "pen" / "undo" / ...
    std::wstring tip;
    RECT hit{};                     // local to toolbar window
    bool checked = false;
    bool hover = false;
};

} // namespace snip
