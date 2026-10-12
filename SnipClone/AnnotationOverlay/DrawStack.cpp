//==============================================================================
// SnipClone — Annotator / undo stack
//==============================================================================
#include "DrawStack.h"

#include <algorithm>
#include <cmath>

namespace snip
{
namespace
{
const D2D1_COLOR_F kPalette[kPaletteCount] = {
    D2D1::ColorF(1.f, 0.25f, 0.25f),
    D2D1::ColorF(1.f, 0.55f, 0.1f),
    D2D1::ColorF(1.f, 0.9f, 0.2f),
    D2D1::ColorF(0.2f, 0.85f, 0.35f),
    D2D1::ColorF(0.2f, 0.55f, 1.f),
    D2D1::ColorF(0.7f, 0.3f, 1.f),
    D2D1::ColorF(1.f, 1.f, 1.f),
    D2D1::ColorF(0.1f, 0.1f, 0.1f),
};
}

DrawStack::DrawStack()
{
    for (int i = 0; i < static_cast<int>(ToolKind::Count); ++i)
    {
        m_styles[i] = {};
    }
    m_styles[static_cast<int>(ToolKind::Text)].fontSize = 20.f;
    m_styles[static_cast<int>(ToolKind::Text)].outline = true;
    m_styles[static_cast<int>(ToolKind::Mosaic)].mosaicSize = 8;
    m_styles[static_cast<int>(ToolKind::Arrow)].arrow = true;
    m_styles[static_cast<int>(ToolKind::Eraser)].width = 16.f;
}

D2D1_COLOR_F DrawStack::PaletteColor(int index)
{
    if (index < 0 || index >= kPaletteCount)
    {
        return kPalette[0];
    }
    return kPalette[index];
}

void DrawStack::SetTool(ToolKind tool)
{
    Finish();
    m_tool = tool;
    if (tool != ToolKind::Select)
    {
        m_selected = -1;
    }
    Notify();
}

void DrawStack::ClearSelection()
{
    m_selected = -1;
    m_movingSelection = false;
}

DrawStyle& DrawStack::MutableStyle(ToolKind tool)
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= static_cast<int>(ToolKind::Count) || tool == ToolKind::Select)
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
}

const DrawStyle& DrawStack::Style(ToolKind tool) const
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= static_cast<int>(ToolKind::Count) || tool == ToolKind::Select)
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
}

DrawStyle* DrawStack::ActiveStyle()
{
    if (m_tool == ToolKind::None || m_tool == ToolKind::Select)
    {
        return nullptr;
    }
    return &MutableStyle(m_tool);
}

void DrawStack::Notify()
{
    if (m_changed)
    {
        m_changed();
    }
}

void DrawStack::PushUndoSnapshot()
{
    m_undo.push_back(m_shapes);
    if (m_undo.size() > 50)
    {
        m_undo.erase(m_undo.begin());
    }
    m_redo.clear();
}

void DrawStack::TranslateShape(DrawCommand& cmd, float dx, float dy)
{
    for (auto& p : cmd.pts)
    {
        p.x += dx;
        p.y += dy;
    }
}

D2D1_RECT_F DrawStack::ApproxBounds(const DrawCommand& cmd) const
{
    if (cmd.pts.empty())
    {
        return D2D1::RectF();
    }
    if (cmd.kind == ToolKind::Text)
    {
        const float w = (std::max)(40.f, cmd.style.fontSize * 0.6f * static_cast<float>(cmd.text.size() + 1));
        const float h = cmd.style.fontSize * 1.4f;
        return D2D1::RectF(cmd.pts[0].x, cmd.pts[0].y, cmd.pts[0].x + w, cmd.pts[0].y + h);
    }
    float minX = cmd.pts[0].x, maxX = cmd.pts[0].x;
    float minY = cmd.pts[0].y, maxY = cmd.pts[0].y;
    for (const auto& p : cmd.pts)
    {
        minX = (std::min)(minX, p.x);
        maxX = (std::max)(maxX, p.x);
        minY = (std::min)(minY, p.y);
        maxY = (std::max)(maxY, p.y);
    }
    const float pad = cmd.style.width + 4.f;
    return D2D1::RectF(minX - pad, minY - pad, maxX + pad, maxY + pad);
}

int DrawStack::HitTest(D2D1_POINT_2F pt) const
{
    for (int i = static_cast<int>(m_shapes.size()) - 1; i >= 0; --i)
    {
        const auto& sh = m_shapes[static_cast<size_t>(i)];
        if (sh.kind == ToolKind::Eraser)
        {
            continue; // eraser strokes are invisible restorations
        }
        const auto b = ApproxBounds(sh);
        if (pt.x >= b.left && pt.x <= b.right && pt.y >= b.top && pt.y <= b.bottom)
        {
            return i;
        }
    }
    return -1;
}

void DrawStack::AddText(D2D1_POINT_2F pt, const std::wstring& text)
{
    if (text.empty())
    {
        return;
    }
    PushUndoSnapshot();
    DrawCommand cmd;
    cmd.kind = ToolKind::Text;
    cmd.pts = { pt };
    cmd.style = Style(ToolKind::Text);
    cmd.text = text;
    m_shapes.push_back(std::move(cmd));
    m_selected = static_cast<int>(m_shapes.size()) - 1;
    Notify();
}

void DrawStack::UpdateText(int index, const std::wstring& text)
{
    if (index < 0 || index >= static_cast<int>(m_shapes.size()))
    {
        return;
    }
    if (m_shapes[static_cast<size_t>(index)].kind != ToolKind::Text)
    {
        return;
    }
    if (text.empty())
    {
        PushUndoSnapshot();
        m_shapes.erase(m_shapes.begin() + index);
        if (m_selected == index)
        {
            m_selected = -1;
        }
        else if (m_selected > index)
        {
            --m_selected;
        }
        Notify();
        return;
    }
    PushUndoSnapshot();
    m_shapes[static_cast<size_t>(index)].text = text;
    Notify();
}

const DrawCommand* DrawStack::ShapeAt(int index) const
{
    if (index < 0 || index >= static_cast<int>(m_shapes.size()))
    {
        return nullptr;
    }
    return &m_shapes[static_cast<size_t>(index)];
}

bool DrawStack::DeleteAt(int index)
{
    if (index < 0 || index >= static_cast<int>(m_shapes.size()))
    {
        return false;
    }
    PushUndoSnapshot();
    m_shapes.erase(m_shapes.begin() + index);
    if (m_selected == index)
    {
        m_selected = -1;
    }
    else if (m_selected > index)
    {
        --m_selected;
    }
    Notify();
    return true;
}

bool DrawStack::DeleteSelected()
{
    return DeleteAt(m_selected);
}

void DrawStack::BeginLive(ToolKind kind, D2D1_POINT_2F pt)
{
    m_live.has = true;
    m_live.cmd = {};
    m_live.cmd.kind = kind;
    m_live.cmd.style = Style(kind);
    m_live.cmd.pts = { pt };
    if (IsTwoPointTool(kind))
    {
        m_live.cmd.pts.push_back(pt);
    }
}

void DrawStack::CommitLive()
{
    if (!m_live.has)
    {
        return;
    }
    if (m_live.cmd.pts.size() < 2)
    {
        m_live = {};
        return;
    }
    PushUndoSnapshot();
    m_shapes.push_back(std::move(m_live.cmd));
    m_selected = static_cast<int>(m_shapes.size()) - 1;
    m_live = {};
}

void DrawStack::Finish()
{
    if (m_dragging || m_live.has)
    {
        m_dragging = false;
        m_movingSelection = false;
        CommitLive();
        Notify();
    }
}

void DrawStack::Press(D2D1_POINT_2F pt)
{
    if (m_tool == ToolKind::None || m_tool == ToolKind::Text)
    {
        return;
    }

    if (m_tool == ToolKind::Select)
    {
        Finish();
        const int hit = HitTest(pt);
        m_selected = hit;
        m_moveUndoPushed = false;
        if (hit >= 0)
        {
            m_movingSelection = true;
            m_dragging = true;
            m_moveOrigin = pt;
        }
        Notify();
        return;
    }

    if (!IsDragShapeTool(m_tool))
    {
        return;
    }
    Finish();
    m_selected = -1;
    m_dragging = true;
    BeginLive(m_tool, pt);
    Notify();
}

void DrawStack::Move(D2D1_POINT_2F pt)
{
    if (!m_dragging)
    {
        return;
    }

    if (m_movingSelection && m_selected >= 0 && m_selected < static_cast<int>(m_shapes.size()))
    {
        const float dx = pt.x - m_moveOrigin.x;
        const float dy = pt.y - m_moveOrigin.y;
        if (dx != 0.f || dy != 0.f)
        {
            if (!m_moveUndoPushed)
            {
                PushUndoSnapshot();
                m_moveUndoPushed = true;
            }
            TranslateShape(m_shapes[static_cast<size_t>(m_selected)], dx, dy);
            m_moveOrigin = pt;
            Notify();
        }
        return;
    }

    if (!m_live.has)
    {
        return;
    }
    if (IsPenLikeTool(m_live.cmd.kind))
    {
        const auto& last = m_live.cmd.pts.back();
        const float dx = pt.x - last.x;
        const float dy = pt.y - last.y;
        if ((dx * dx + dy * dy) >= 0.25f)
        {
            m_live.cmd.pts.push_back(pt);
            Notify();
        }
    }
    else
    {
        m_live.cmd.pts.back() = pt;
        Notify();
    }
}

void DrawStack::Release(D2D1_POINT_2F pt)
{
    if (!m_dragging)
    {
        return;
    }
    Move(pt);
    m_dragging = false;
    if (m_movingSelection)
    {
        m_movingSelection = false;
        // Drop no-op move undo: if last undo equals current, pop — skip for simplicity.
        Notify();
        return;
    }
    CommitLive();
    Notify();
}

void DrawStack::AdjustWidth(int delta)
{
    auto* st = ActiveStyle();
    if (!st || m_tool == ToolKind::Text || m_tool == ToolKind::Mosaic)
    {
        return;
    }
    st->width = (std::max)(1.f, (std::min)(40.f, st->width + static_cast<float>(delta)));
    if (m_live.has && m_live.cmd.kind == m_tool)
    {
        m_live.cmd.style.width = st->width;
    }
    Notify();
}

void DrawStack::AdjustFontSize(int delta)
{
    auto& st = MutableStyle(ToolKind::Text);
    st.fontSize = (std::max)(8.f, (std::min)(120.f, st.fontSize + static_cast<float>(delta) * 2.f));
    Notify();
}

void DrawStack::AdjustMosaicSize(int delta)
{
    auto& st = MutableStyle(ToolKind::Mosaic);
    st.mosaicSize = (std::max)(2, (std::min)(40, st.mosaicSize + delta));
    if (m_live.has && m_live.cmd.kind == ToolKind::Mosaic)
    {
        m_live.cmd.style.mosaicSize = st.mosaicSize;
    }
    Notify();
}

void DrawStack::CycleColor()
{
    SetColorIndex((m_colorIndex + 1) % kPaletteCount);
}

void DrawStack::SetColorIndex(int index)
{
    if (index < 0 || index >= kPaletteCount)
    {
        return;
    }
    m_colorIndex = index;
    auto* st = ActiveStyle();
    if (!st || m_tool == ToolKind::Mosaic || m_tool == ToolKind::Eraser)
    {
        // Still apply to pen/text/rect styles commonly used.
        for (ToolKind k : { ToolKind::Pen, ToolKind::Rect, ToolKind::Ellipse, ToolKind::Arrow, ToolKind::Text })
        {
            MutableStyle(k).color = kPalette[m_colorIndex];
        }
        Notify();
        return;
    }
    st->color = kPalette[m_colorIndex];
    if (m_live.has)
    {
        m_live.cmd.style.color = st->color;
    }
    Notify();
}

void DrawStack::ToggleBold()
{
    auto& st = MutableStyle(ToolKind::Text);
    st.bold = !st.bold;
    Notify();
}

void DrawStack::ToggleItalic()
{
    auto& st = MutableStyle(ToolKind::Text);
    st.italic = !st.italic;
    Notify();
}

void DrawStack::ToggleOutline()
{
    auto& st = MutableStyle(ToolKind::Text);
    st.outline = !st.outline;
    Notify();
}

void DrawStack::ToggleFilled()
{
    auto& rect = MutableStyle(ToolKind::Rect);
    rect.filled = !rect.filled;
    MutableStyle(ToolKind::Ellipse).filled = rect.filled;
    if (m_live.has && ToolUsesFill(m_live.cmd.kind))
    {
        m_live.cmd.style.filled = rect.filled;
    }
    Notify();
}

void DrawStack::Undo()
{
    Finish();
    if (m_undo.empty())
    {
        return;
    }
    m_redo.push_back(m_shapes);
    m_shapes = std::move(m_undo.back());
    m_undo.pop_back();
    m_selected = -1;
    Notify();
}

void DrawStack::Redo()
{
    Finish();
    if (m_redo.empty())
    {
        return;
    }
    m_undo.push_back(m_shapes);
    m_shapes = std::move(m_redo.back());
    m_redo.pop_back();
    m_selected = -1;
    Notify();
}

void DrawStack::Clear()
{
    Finish();
    m_undo.clear();
    m_redo.clear();
    m_shapes.clear();
    m_live = {};
    m_selected = -1;
    Notify();
}

} // namespace snip
