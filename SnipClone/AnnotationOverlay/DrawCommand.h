//==============================================================================
// SnipClone — vector draw commands (PinShot Shape 对位)
//==============================================================================
#pragma once

#include <Windows.h>
#include <d2d1.h>

#include <vector>

namespace snip
{

enum class ToolKind
{
    None = 0,
    Pen,
    Rect,
    Arrow,
};

struct DrawStyle
{
    D2D1_COLOR_F color{ 1.f, 0.2f, 0.2f, 1.f };
    float width = 3.f;
    bool arrow = true;
};

// Mirrors PinShot Shape: kind + pts + style.
struct DrawCommand
{
    ToolKind kind = ToolKind::None;
    std::vector<D2D1_POINT_2F> pts; // image-local pixels
    DrawStyle style{};
};

inline const wchar_t* ToolKindName(ToolKind k)
{
    switch (k)
    {
    case ToolKind::Pen:
        return L"pen";
    case ToolKind::Rect:
        return L"rect";
    case ToolKind::Arrow:
        return L"arrow";
    default:
        return L"none";
    }
}

} // namespace snip
