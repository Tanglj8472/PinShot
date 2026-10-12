//==============================================================================
// SnipClone — vector draw commands（对齐 PinShot Shape）
//==============================================================================
#pragma once

#include "../Common/WinCompat.h"
#include <d2d1.h>

#include <string>
#include <vector>

namespace snip
{

enum class ToolKind
{
    None = 0,
    Pen,
    Rect,
    Ellipse,
    Arrow,
    Text,
    Mosaic,
    Eraser,
    Select, // meta-tool: hit / move / delete shapes
    Count
};

struct DrawStyle
{
    D2D1_COLOR_F color{ 1.f, 0.25f, 0.25f, 1.f };
    float width = 3.f;
    bool arrow = true;
    bool filled = false;
    float fontSize = 20.f;
    int mosaicSize = 8;
    bool bold = false;
    bool italic = false;
    bool outline = true;
};

struct DrawCommand
{
    ToolKind kind = ToolKind::None;
    std::vector<D2D1_POINT_2F> pts; // image-local pixels
    DrawStyle style{};
    std::wstring text;
};

inline const wchar_t* ToolKindName(ToolKind k)
{
    switch (k)
    {
    case ToolKind::Pen:
        return L"pen";
    case ToolKind::Rect:
        return L"rect";
    case ToolKind::Ellipse:
        return L"ellip";
    case ToolKind::Arrow:
        return L"arrow";
    case ToolKind::Text:
        return L"text";
    case ToolKind::Mosaic:
        return L"mos";
    case ToolKind::Eraser:
        return L"eras";
    case ToolKind::Select:
        return L"sel";
    default:
        return L"none";
    }
}

inline bool IsPenLikeTool(ToolKind k)
{
    return k == ToolKind::Pen || k == ToolKind::Eraser;
}

inline bool IsDragShapeTool(ToolKind k)
{
    return k == ToolKind::Pen || k == ToolKind::Rect || k == ToolKind::Ellipse ||
           k == ToolKind::Arrow || k == ToolKind::Mosaic || k == ToolKind::Eraser;
}

inline bool IsTwoPointTool(ToolKind k)
{
    return k == ToolKind::Rect || k == ToolKind::Ellipse || k == ToolKind::Arrow ||
           k == ToolKind::Mosaic;
}

inline bool ToolUsesColor(ToolKind k)
{
    return k == ToolKind::Pen || k == ToolKind::Rect || k == ToolKind::Ellipse ||
           k == ToolKind::Arrow || k == ToolKind::Text;
}

inline bool ToolUsesFill(ToolKind k)
{
    return k == ToolKind::Rect || k == ToolKind::Ellipse;
}

} // namespace snip
