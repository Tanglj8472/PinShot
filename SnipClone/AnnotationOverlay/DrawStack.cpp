//==============================================================================
// SnipClone — Annotator / undo stack
//==============================================================================
#include "DrawStack.h"

namespace snip
{

DrawStack::DrawStack()
{
    m_styles[static_cast<int>(ToolKind::Pen)] = { D2D1::ColorF(1.f, 0.25f, 0.25f), 3.f, false };
    m_styles[static_cast<int>(ToolKind::Rect)] = { D2D1::ColorF(1.f, 0.25f, 0.25f), 3.f, false };
    m_styles[static_cast<int>(ToolKind::Arrow)] = { D2D1::ColorF(1.f, 0.25f, 0.25f), 3.f, true };
}

void DrawStack::SetTool(ToolKind tool)
{
    Finish();
    m_tool = tool;
    Notify();
}

DrawStyle& DrawStack::MutableStyle(ToolKind tool)
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= 4)
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
}

const DrawStyle& DrawStack::Style(ToolKind tool) const
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= 4)
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
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

void DrawStack::BeginLive(ToolKind kind, D2D1_POINT_2F pt)
{
    m_live.has = true;
    m_live.cmd = {};
    m_live.cmd.kind = kind;
    m_live.cmd.style = Style(kind);
    m_live.cmd.pts = { pt };
    if (kind != ToolKind::Pen)
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
    m_live = {};
}

void DrawStack::Finish()
{
    if (m_dragging || m_live.has)
    {
        m_dragging = false;
        CommitLive();
        Notify();
    }
}

void DrawStack::Press(D2D1_POINT_2F pt)
{
    if (m_tool == ToolKind::None)
    {
        return;
    }
    Finish();
    m_dragging = true;
    BeginLive(m_tool, pt);
    Notify();
}

void DrawStack::Move(D2D1_POINT_2F pt)
{
    if (!m_dragging || !m_live.has)
    {
        return;
    }

    if (m_live.cmd.kind == ToolKind::Pen)
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
    CommitLive();
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
    Notify();
}

void DrawStack::Clear()
{
    Finish();
    m_undo.clear();
    m_redo.clear();
    m_shapes.clear();
    m_live = {};
    Notify();
}

} // namespace snip
