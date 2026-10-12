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

namespace
{
float Dist2(D2D1_POINT_2F a, D2D1_POINT_2F b)
{
    const float dx = a.x - b.x;
    const float dy = a.y - b.y;
    return dx * dx + dy * dy;
}

float DistPointSeg(D2D1_POINT_2F p, D2D1_POINT_2F a, D2D1_POINT_2F b)
{
    const float dx = b.x - a.x;
    const float dy = b.y - a.y;
    const float len2 = dx * dx + dy * dy;
    if (len2 < 1e-6f)
    {
        return std::sqrt(Dist2(p, a));
    }
    float t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    t = (std::max)(0.f, (std::min)(1.f, t));
    return std::sqrt(Dist2(p, D2D1::Point2F(a.x + t * dx, a.y + t * dy)));
}

float DistPointPolyline(D2D1_POINT_2F p, const std::vector<D2D1_POINT_2F>& pts)
{
    if (pts.empty())
    {
        return 1e9f;
    }
    if (pts.size() == 1)
    {
        return std::sqrt(Dist2(p, pts[0]));
    }
    float best = 1e9f;
    for (size_t i = 1; i < pts.size(); ++i)
    {
        best = (std::min)(best, DistPointSeg(p, pts[i - 1], pts[i]));
    }
    return best;
}

bool PointInEllipse(D2D1_POINT_2F p, D2D1_POINT_2F a, D2D1_POINT_2F b)
{
    const float cx = (a.x + b.x) * 0.5f;
    const float cy = (a.y + b.y) * 0.5f;
    const float rx = std::abs(b.x - a.x) * 0.5f;
    const float ry = std::abs(b.y - a.y) * 0.5f;
    if (rx < 1.f || ry < 1.f)
    {
        return DistPointSeg(p, a, b) <= 6.f;
    }
    const float nx = (p.x - cx) / rx;
    const float ny = (p.y - cy) / ry;
    return (nx * nx + ny * ny) <= 1.f;
}
} // namespace

D2D1_RECT_F DrawStack::ApproxBounds(const DrawCommand& cmd) const
{
    if (cmd.pts.empty())
    {
        return D2D1::RectF();
    }
    if (cmd.kind == ToolKind::Text)
    {
        // Prefer wider CJK-friendly estimate (~1em per glyph).
        const float w = (std::max)(40.f, cmd.style.fontSize * 1.05f * static_cast<float>(cmd.text.size() + 1));
        const float h = cmd.style.fontSize * 1.5f;
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

bool DrawStack::HitShape(const DrawCommand& cmd, D2D1_POINT_2F pt) const
{
    if (cmd.pts.empty() || cmd.kind == ToolKind::Eraser)
    {
        return false;
    }
    const float tol = (std::max)(6.f, cmd.style.width * 0.5f + 4.f);

    if (cmd.kind == ToolKind::Text)
    {
        const auto b = ApproxBounds(cmd);
        return pt.x >= b.left && pt.x <= b.right && pt.y >= b.top && pt.y <= b.bottom;
    }
    if (cmd.kind == ToolKind::Pen || cmd.kind == ToolKind::Mosaic)
    {
        return DistPointPolyline(pt, cmd.pts) <= tol;
    }
    if (cmd.kind == ToolKind::Arrow && cmd.pts.size() >= 2)
    {
        return DistPointSeg(pt, cmd.pts.front(), cmd.pts.back()) <= tol;
    }
    if (cmd.kind == ToolKind::Rect && cmd.pts.size() >= 2)
    {
        const float l = (std::min)(cmd.pts[0].x, cmd.pts[1].x);
        const float r = (std::max)(cmd.pts[0].x, cmd.pts[1].x);
        const float t = (std::min)(cmd.pts[0].y, cmd.pts[1].y);
        const float b = (std::max)(cmd.pts[0].y, cmd.pts[1].y);
        if (cmd.style.filled)
        {
            return pt.x >= l && pt.x <= r && pt.y >= t && pt.y <= b;
        }
        const bool nearEdge =
            (pt.x >= l - tol && pt.x <= r + tol && pt.y >= t - tol && pt.y <= t + tol) ||
            (pt.x >= l - tol && pt.x <= r + tol && pt.y >= b - tol && pt.y <= b + tol) ||
            (pt.y >= t - tol && pt.y <= b + tol && pt.x >= l - tol && pt.x <= l + tol) ||
            (pt.y >= t - tol && pt.y <= b + tol && pt.x >= r - tol && pt.x <= r + tol);
        return nearEdge;
    }
    if (cmd.kind == ToolKind::Ellipse && cmd.pts.size() >= 2)
    {
        if (cmd.style.filled)
        {
            return PointInEllipse(pt, cmd.pts[0], cmd.pts[1]);
        }
        // Ring hit: inside inflated ellipse, outside deflated.
        const float cx = (cmd.pts[0].x + cmd.pts[1].x) * 0.5f;
        const float cy = (cmd.pts[0].y + cmd.pts[1].y) * 0.5f;
        float rx = std::abs(cmd.pts[1].x - cmd.pts[0].x) * 0.5f;
        float ry = std::abs(cmd.pts[1].y - cmd.pts[0].y) * 0.5f;
        if (rx < 1.f || ry < 1.f)
        {
            return DistPointSeg(pt, cmd.pts[0], cmd.pts[1]) <= tol;
        }
        const float nx = (pt.x - cx) / rx;
        const float ny = (pt.y - cy) / ry;
        const float d = std::sqrt(nx * nx + ny * ny);
        const float band = tol / (std::min)(rx, ry);
        return std::abs(d - 1.f) <= band;
    }
    const auto b = ApproxBounds(cmd);
    return pt.x >= b.left && pt.x <= b.right && pt.y >= b.top && pt.y <= b.bottom;
}

int DrawStack::HitTest(D2D1_POINT_2F pt) const
{
    for (int i = static_cast<int>(m_shapes.size()) - 1; i >= 0; --i)
    {
        if (HitShape(m_shapes[static_cast<size_t>(i)], pt))
        {
            return i;
        }
    }
    return -1;
}

int DrawStack::HitResizeHandle(int shapeIndex, D2D1_POINT_2F pt) const
{
    if (shapeIndex < 0 || shapeIndex >= static_cast<int>(m_shapes.size()))
    {
        return -1;
    }
    const auto& sh = m_shapes[static_cast<size_t>(shapeIndex)];
    if (!IsTwoPointTool(sh.kind) || sh.pts.size() < 2)
    {
        return -1;
    }
    constexpr float kHandle = 8.f;
    if (std::sqrt(Dist2(pt, sh.pts[0])) <= kHandle)
    {
        return 1;
    }
    if (std::sqrt(Dist2(pt, sh.pts[1])) <= kHandle)
    {
        return 2;
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

void DrawStack::UpdateText(int index, const std::wstring& text, const DrawStyle* style)
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
    if (style)
    {
        m_shapes[static_cast<size_t>(index)].style = *style;
    }
    Notify();
}

void DrawStack::RemapForCrop(float dx, float dy, float left, float top, float right, float bottom)
{
    PushUndoSnapshot();
    TranslateAll(dx, dy);
    CullOutside(left, top, right, bottom);
    Notify();
}

void DrawStack::TranslateAll(float dx, float dy)
{
    if (dx == 0.f && dy == 0.f)
    {
        return;
    }
    for (auto& sh : m_shapes)
    {
        TranslateShape(sh, dx, dy);
    }
}

void DrawStack::CullOutside(float left, float top, float right, float bottom)
{
    std::vector<DrawCommand> kept;
    kept.reserve(m_shapes.size());
    for (auto& sh : m_shapes)
    {
        const auto b = ApproxBounds(sh);
        const bool outside = b.right < left || b.left > right || b.bottom < top || b.top > bottom;
        if (!outside)
        {
            kept.push_back(std::move(sh));
        }
    }
    m_shapes = std::move(kept);
    m_selected = -1;
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
        m_resizingSelection = false;
        m_resizeHandle = 0;
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
        m_resizingSelection = false;
        m_resizeHandle = 0;
        m_moveUndoPushed = false;

        // Prefer resize handles on the current selection.
        if (m_selected >= 0)
        {
            const int handle = HitResizeHandle(m_selected, pt);
            if (handle > 0)
            {
                m_resizingSelection = true;
                m_resizeHandle = handle;
                m_dragging = true;
                m_moveOrigin = pt;
                Notify();
                return;
            }
        }

        const int hit = HitTest(pt);
        m_selected = hit;
        if (hit >= 0)
        {
            const int handle = HitResizeHandle(hit, pt);
            if (handle > 0)
            {
                m_resizingSelection = true;
                m_resizeHandle = handle;
            }
            else
            {
                m_movingSelection = true;
            }
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

    if (m_resizingSelection && m_selected >= 0 && m_selected < static_cast<int>(m_shapes.size()) &&
        m_resizeHandle > 0)
    {
        auto& sh = m_shapes[static_cast<size_t>(m_selected)];
        if (sh.pts.size() >= 2)
        {
            if (!m_moveUndoPushed)
            {
                PushUndoSnapshot();
                m_moveUndoPushed = true;
            }
            const size_t idx = static_cast<size_t>(m_resizeHandle == 1 ? 0 : 1);
            sh.pts[idx] = pt;
            m_moveOrigin = pt;
            Notify();
        }
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
    if (m_movingSelection || m_resizingSelection)
    {
        m_movingSelection = false;
        m_resizingSelection = false;
        m_resizeHandle = 0;
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
    SetColorRgb(kPalette[index].r, kPalette[index].g, kPalette[index].b);
}

void DrawStack::SetColorRgb(float r, float g, float b)
{
    const D2D1_COLOR_F c = D2D1::ColorF(r, g, b, 1.f);
    for (ToolKind k : { ToolKind::Pen, ToolKind::Rect, ToolKind::Ellipse, ToolKind::Arrow, ToolKind::Text })
    {
        MutableStyle(k).color = c;
    }
    if (m_live.has && ToolUsesColor(m_live.cmd.kind))
    {
        m_live.cmd.style.color = c;
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
