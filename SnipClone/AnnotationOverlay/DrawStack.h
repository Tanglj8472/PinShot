//==============================================================================
// SnipClone — Annotator / undo stack（对齐 PinShot Annotator）
//==============================================================================
#pragma once

#include "DrawCommand.h"

#include <functional>
#include <string>
#include <vector>

namespace snip
{

class DrawStack
{
public:
    using ChangedFn = std::function<void()>;

    DrawStack();

    void SetChangedCallback(ChangedFn fn) { m_changed = std::move(fn); }

    ToolKind Tool() const { return m_tool; }
    void SetTool(ToolKind tool);

    DrawStyle& MutableStyle(ToolKind tool);
    const DrawStyle& Style(ToolKind tool) const;
    DrawStyle* ActiveStyle();

    const std::vector<DrawCommand>& Shapes() const { return m_shapes; }
    const DrawCommand* Live() const { return m_live.has ? &m_live.cmd : nullptr; }

    void Press(D2D1_POINT_2F pt);
    void Move(D2D1_POINT_2F pt);
    void Release(D2D1_POINT_2F pt);
    void Finish();

    // Text shapes — content comes from floating TextEditor (Snipaste-style).
    int HitTest(D2D1_POINT_2F pt) const; // -1 = miss; prefers text
    void AddText(D2D1_POINT_2F pt, const std::wstring& text);
    void UpdateText(int index, const std::wstring& text);
    const DrawCommand* ShapeAt(int index) const;

    void AdjustWidth(int delta);
    void AdjustFontSize(int delta);
    void AdjustMosaicSize(int delta);
    void CycleColor();
    void ToggleBold();
    void ToggleItalic();
    void ToggleOutline();

    void Undo();
    void Redo();
    void Clear();

    bool CanUndo() const { return !m_undo.empty(); }
    bool CanRedo() const { return !m_redo.empty(); }

private:
    void Notify();
    void PushUndoSnapshot();
    void BeginLive(ToolKind kind, D2D1_POINT_2F pt);
    void CommitLive();
    D2D1_RECT_F ApproxBounds(const DrawCommand& cmd) const;

    ToolKind m_tool = ToolKind::Pen;
    DrawStyle m_styles[static_cast<int>(ToolKind::Count)]{};
    std::vector<DrawCommand> m_shapes;
    std::vector<std::vector<DrawCommand>> m_undo;
    std::vector<std::vector<DrawCommand>> m_redo;

    struct LiveState
    {
        bool has = false;
        DrawCommand cmd{};
    } m_live;

    bool m_dragging = false;
    int m_colorIndex = 0;
    ChangedFn m_changed;
};

} // namespace snip
