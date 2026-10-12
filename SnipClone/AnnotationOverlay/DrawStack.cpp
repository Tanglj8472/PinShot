//==============================================================================
// SnipClone — Annotator / undo stack
//==============================================================================
#include "DrawStack.h"

#include <algorithm>

namespace snip
{
namespace
{
const D2D1_COLOR_F kPalette[] = {
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
    m_styles[static_cast<int>(ToolKind::Pen)].width = 3.f;
    m_styles[static_cast<int>(ToolKind::Rect)].width = 3.f;
    m_styles[static_cast<int>(ToolKind::Ellipse)].width = 3.f;
    m_styles[static_cast<int>(ToolKind::Arrow)].width = 3.f;
    m_styles[static_cast<int>(ToolKind::Arrow)].arrow = true;
    m_styles[static_cast<int>(ToolKind::Text)].fontSize = 22.f;
    m_styles[static_cast<int>(ToolKind::Mosaic)].mosaicSize = 8;
}

void DrawStack::SetTool(ToolKind tool)
{
    if (m_editingText)
    {
        CommitText(false);
    }
    Finish();
    m_tool = tool;
    Notify();
}

DrawStyle& DrawStack::MutableStyle(ToolKind tool)
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= static_cast<int>(ToolKind::Count))
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
}

const DrawStyle& DrawStack::Style(ToolKind tool) const
{
    const int i = static_cast<int>(tool);
    if (i <= 0 || i >= static_cast<int>(ToolKind::Count))
    {
        return m_styles[static_cast<int>(ToolKind::Pen)];
    }
    return m_styles[i];
}

DrawStyle* DrawStack::ActiveStyle()
{
    if (m_tool == ToolKind::None)
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

void DrawStack::BeginTextAt(D2D1_POINT_2F pt)
{
    Finish();
    m_live.has = true;
    m_live.cmd = {};
    m_live.cmd.kind = ToolKind::Text;
    m_live.cmd.style = Style(ToolKind::Text);
    m_live.cmd.pts = { pt };
    m_live.cmd.text = L"";
    m_editingText = true;
    Notify();
}

void DrawStack::CommitLive()
{
    if (!m_live.has)
    {
        return;
    }
    if (m_live.cmd.kind == ToolKind::Text)
    {
        if (m_live.cmd.text.empty())
        {
            m_live = {};
            m_editingText = false;
            return;
        }
        PushUndoSnapshot();
        m_shapes.push_back(std::move(m_live.cmd));
        m_live = {};
        m_editingText = false;
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
    if (m_editingText)
    {
        return; // text commits explicitly
    }
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
    if (m_tool == ToolKind::Text)
    {
        if (m_editingText)
        {
            CommitText(false);
        }
        BeginTextAt(pt);
        return;
    }
    if (!IsDragShapeTool(m_tool))
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
    if (m_editingText || !m_dragging || !m_live.has)
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
    if (m_editingText || !m_dragging)
    {
        return;
    }
    Move(pt);
    m_dragging = false;
    CommitLive();
    Notify();
}

void DrawStack::AppendTextChar(wchar_t ch)
{
    if (!m_editingText || !m_live.has)
    {
        return;
    }
    if (ch == L'\r' || ch == L'\n')
    {
        CommitText(false);
        return;
    }
    if (ch == 8) // backspace
    {
        if (!m_live.cmd.text.empty())
        {
            m_live.cmd.text.pop_back();
            Notify();
        }
        return;
    }
    if (ch >= 32)
    {
        m_live.cmd.text.push_back(ch);
        Notify();
    }
}

void DrawStack::CommitText(bool cancel)
{
    if (!m_editingText)
    {
        return;
    }
    if (cancel)
    {
        m_live = {};
        m_editingText = false;
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
    st.fontSize = (std::max)(10.f, (std::min)(96.f, st.fontSize + static_cast<float>(delta) * 2.f));
    if (m_live.has && m_live.cmd.kind == ToolKind::Text)
    {
        m_live.cmd.style.fontSize = st.fontSize;
    }
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
    m_colorIndex = (m_colorIndex + 1) % static_cast<int>(sizeof(kPalette) / sizeof(kPalette[0]));
    auto* st = ActiveStyle();
    if (!st || m_tool == ToolKind::Mosaic)
    {
        return;
    }
    st->color = kPalette[m_colorIndex];
    if (m_live.has)
    {
        m_live.cmd.style.color = st->color;
    }
    Notify();
}

void DrawStack::Undo()
{
    if (m_editingText)
    {
        CommitText(true);
        return;
    }
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
    if (m_editingText)
    {
        return;
    }
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
    if (m_editingText)
    {
        CommitText(true);
    }
    Finish();
    m_undo.clear();
    m_redo.clear();
    m_shapes.clear();
    m_live = {};
    Notify();
}

} // namespace snip
