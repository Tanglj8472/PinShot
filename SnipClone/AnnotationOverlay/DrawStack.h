//==============================================================================
// SnipClone — Annotator / undo stack（对齐 PinShot Annotator）
//==============================================================================
#pragma once

#include "DrawCommand.h"

#include "../Common/WinCompat.h"

#include <functional>
#include <string>
#include <vector>

namespace snip
{

constexpr int kPaletteCount = 8;

class DrawStack
{
public:
    using ChangedFn = std::function<void()>;

    // One undo step: shapes + optional crop viewport (A01 unified document history).
    struct HistoryFrame
    {
        std::vector<DrawCommand> shapes;
        RECT crop{};
        bool hasCrop = false;
    };

    DrawStack();

    void SetChangedCallback(ChangedFn fn) { m_changed = std::move(fn); }

    // Bind current crop viewport so every undo frame can restore it with shapes.
    void SetViewport(RECT cropScreen);
    RECT Viewport() const { return m_viewport; }
    bool HasViewport() const { return m_viewportBound; }

    ToolKind Tool() const { return m_tool; }
    void SetTool(ToolKind tool);

    DrawStyle& MutableStyle(ToolKind tool);
    const DrawStyle& Style(ToolKind tool) const;
    DrawStyle* ActiveStyle();
    // M1: selected shape style when present; otherwise current tool preset (may be null).
    DrawStyle* EditTargetStyle();
    const DrawStyle* EditTargetStyle() const;
    // Kind of the style edit target (selected shape kind, else current tool).
    ToolKind EditTargetKind() const;

    const std::vector<DrawCommand>& Shapes() const { return m_shapes; }
    const DrawCommand* Live() const { return m_live.has ? &m_live.cmd : nullptr; }

    int SelectedIndex() const { return m_selected; }
    void ClearSelection();
    bool DuplicateSelected(); // Ctrl+D — object copy, not whole-image copy

    void Press(D2D1_POINT_2F pt);
    void Move(D2D1_POINT_2F pt);
    void Release(D2D1_POINT_2F pt);
    void Finish();

    // Hit any shape (top-most). Distance tests for pen/arrow; AABB/ellipse for fills.
    int HitTest(D2D1_POINT_2F pt) const;
    // When a shape is selected: -1 none, 0 body/move, 1..n resize handles (two-point: 1=p0, 2=p1).
    int HitResizeHandle(int shapeIndex, D2D1_POINT_2F pt) const;
    void AddText(D2D1_POINT_2F pt, const std::wstring& text);
    // style: if non-null, replace shape style (re-edit / toolbar sync on commit).
    void UpdateText(int index, const std::wstring& text, const DrawStyle* style = nullptr);
    const DrawCommand* ShapeAt(int index) const;
    bool DeleteSelected();
    bool DeleteAt(int index);

    // Crop origin moved by (dx,dy) in image space: translate all shapes once (undoable).
    // Always pushes history (even if dx=dy=0) so edge-only crop changes are undoable.
    // Does NOT delete shapes — outside the new crop is clipped at render and returns if crop grows.
    void RemapForCrop(float dx, float dy);
    void TranslateAll(float dx, float dy);
    void CullOutside(float left, float top, float right, float bottom); // optional hard delete

    void AdjustWidth(int delta);
    void AdjustFontSize(int delta);
    void AdjustMosaicSize(int delta);
    void CycleColor();
    void SetColorIndex(int index);
    void SetColorRgb(float r, float g, float b);
    int ColorIndex() const { return m_colorIndex; }
    static D2D1_COLOR_F PaletteColor(int index);
    void ToggleBold();
    void ToggleItalic();
    void ToggleOutline();
    void ToggleFilled();

    // Returns true if a step was applied. When outHasCrop is set and the frame
    // stored a viewport, *outHasCrop=true and *outCrop receives that RECT.
    bool Undo(RECT* outCrop = nullptr, bool* outHasCrop = nullptr);
    bool Redo(RECT* outCrop = nullptr, bool* outHasCrop = nullptr);
    void Clear();

    bool CanUndo() const { return !m_undo.empty(); }
    bool CanRedo() const { return !m_redo.empty(); }

private:
    void Notify();
    void PushUndoSnapshot();
    void BeginLive(ToolKind kind, D2D1_POINT_2F pt);
    void CommitLive();
    D2D1_RECT_F ApproxBounds(const DrawCommand& cmd) const;
    bool HitShape(const DrawCommand& cmd, D2D1_POINT_2F pt) const;
    void TranslateShape(DrawCommand& cmd, float dx, float dy);

    ToolKind m_tool = ToolKind::Pen;
    DrawStyle m_styles[static_cast<int>(ToolKind::Count)]{};
    std::vector<DrawCommand> m_shapes;
    std::vector<HistoryFrame> m_undo;
    std::vector<HistoryFrame> m_redo;

    RECT m_viewport{};
    bool m_viewportBound = false;

    struct LiveState
    {
        bool has = false;
        DrawCommand cmd{};
    } m_live;

    bool m_dragging = false;
    int m_colorIndex = 0;
    int m_selected = -1;
    bool m_movingSelection = false;
    bool m_resizingSelection = false;
    int m_resizeHandle = 0; // 1 = first point / NW-ish, 2 = second point
    bool m_moveUndoPushed = false;
    D2D1_POINT_2F m_moveOrigin{};
    ChangedFn m_changed;
};

} // namespace snip
