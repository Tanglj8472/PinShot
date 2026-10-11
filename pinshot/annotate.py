"""Shared annotation engine: shapes, styles, rendering, input, select/edit."""
import math
import time
from copy import copy, deepcopy
from dataclasses import dataclass, field
from PySide6.QtCore import Qt, QPointF, QRectF, QObject, Signal
from PySide6.QtGui import (QPainter, QPen, QColor, QPolygonF, QFont, QFontMetricsF, QPainterPath,
                           QPainterPathStroker, QImage)
from PySide6.QtWidgets import QLineEdit

# "select" is a meta-tool (not drawn); "number" = numbered markers
TOOLS = ["select", "rect", "ellipse", "line", "pen", "marker", "mosaic", "text", "number", "eraser"]
DRAW_TOOLS = [t for t in TOOLS if t != "select"]
PALETTE = ["#ff0000", "#ff8800", "#ffd400", "#55cc00", "#00aa55", "#00c8c8", "#0088ff", "#2040ff", "#8830ff", "#ff40c0",
           "#ffffff", "#c0c0c0", "#808080", "#404040", "#000000", "#ffb0b0", "#ffe0a0", "#b0f0b0", "#a0d8ff", "#e0b0ff"]


@dataclass
class Style:
    color: QColor = field(default_factory=lambda: QColor("#ff0000"))
    width: int = 3
    filled: bool = False
    arrow: bool = True
    line_style: str = "solid"
    mode: str = "rect"
    effect: str = "mosaic"
    size: int = 6
    bold: bool = False
    italic: bool = False
    outline: bool = True
    family: str = ""
    font_size: int = 20


def default_styles():
    s = {t: Style() for t in DRAW_TOOLS}
    s["marker"].color = QColor("#ffd400"); s["marker"].width = 14
    s["pen"].width = 3; s["line"].width = 3
    s["eraser"].mode = "brush"; s["eraser"].width = 16
    s["mosaic"].mode = "brush"
    s["number"].width = 22; s["number"].filled = True; s["number"].font_size = 14
    return s


@dataclass
class Shape:
    kind: str
    pts: list
    style: Style
    text: str = ""
    number: int = 0          # for kind=="number"


PEN_STYLES = {"solid": Qt.SolidLine, "dash": Qt.DashLine, "dot": Qt.DotLine}
HANDLE = 5


def _rect(sh):
    if sh.kind == "number" and len(sh.pts) >= 1:
        r = max(8, sh.style.width) / 2
        c = sh.pts[0]
        return QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r)
    if sh.kind == "text" and sh.text:
        f = text_font(sh.style); fm = QFontMetricsF(f)
        lines = sh.text.split("\n"); w = max(fm.horizontalAdvance(L) for L in lines)
        h = fm.lineSpacing() * len(lines)
        return QRectF(sh.pts[0].x(), sh.pts[0].y(), w + 4, h + 2)
    if len(sh.pts) < 2:
        p = sh.pts[0] if sh.pts else QPointF()
        return QRectF(p.x() - 2, p.y() - 2, 4, 4)
    return QRectF(sh.pts[0], sh.pts[-1]).normalized()


def shape_bounds(sh) -> QRectF:
    if sh.kind in ("pen", "marker", "line") and len(sh.pts) > 2:
        xs = [p.x() for p in sh.pts]; ys = [p.y() for p in sh.pts]
        return QRectF(QPointF(min(xs), min(ys)), QPointF(max(xs), max(ys))).normalized()
    return _rect(sh)


def _stroke_path(pts, width):
    path = QPainterPath(pts[0])
    for q in pts[1:]: path.lineTo(q)
    if len(pts) == 1: path.lineTo(pts[0] + QPointF(0.1, 0))
    st = QPainterPathStroker(); st.setWidth(width); st.setCapStyle(Qt.RoundCap); st.setJoinStyle(Qt.RoundJoin)
    return st.createStroke(path)


class EffectCache:
    def __init__(self): self.base_key = None; self.cache = {}

    def get(self, base: QImage, effect: str, size: int) -> QImage:
        key = (base.cacheKey(), base.width(), base.height())
        if key != self.base_key: self.base_key = key; self.cache = {}
        k = (effect, size)
        if k not in self.cache:
            w, h = base.width(), base.height()
            if effect == "blur":
                f = max(2, size)
                small = base.scaled(max(1, w // f), max(1, h // f), Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
                small = small.scaled(max(1, w // (f // 2 or 1)), max(1, h // (f // 2 or 1)), Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
                img = small.scaled(w, h, Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
            else:
                b = max(2, size * 2)
                small = base.scaled(max(1, w // b), max(1, h // b), Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
                img = small.scaled(w, h, Qt.IgnoreAspectRatio, Qt.FastTransformation)
            self.cache[k] = img
        return self.cache[k]


def render(p: QPainter, shapes, base: QImage, cache: EffectCache, selected=None):
    p.save(); p.setRenderHint(QPainter.Antialiasing)
    for sh in shapes:
        st = sh.style
        if not sh.pts: continue
        if sh.kind in ("mosaic", "eraser"):
            if st.mode == "brush":
                region = _stroke_path(sh.pts, (st.size * 6) if sh.kind == "mosaic" else st.width)
            else:
                region = QPainterPath(); region.addRect(_rect(sh))
            src = base if sh.kind == "eraser" else cache.get(base, st.effect, st.size)
            p.save(); p.setClipPath(region, Qt.IntersectClip); p.drawImage(0, 0, src); p.restore()
            continue
        pen = QPen(st.color, st.width, PEN_STYLES.get(st.line_style, Qt.SolidLine), Qt.RoundCap, Qt.RoundJoin)
        p.setPen(pen); p.setBrush(Qt.NoBrush)
        if sh.kind in ("rect", "ellipse"):
            if st.filled:
                p.setCompositionMode(QPainter.CompositionMode_SourceOver)
                p.setBrush(st.color); p.setPen(Qt.NoPen)
            (p.drawRect if sh.kind == "rect" else p.drawEllipse)(_rect(sh))
        elif sh.kind == "line":
            path = QPainterPath(sh.pts[0])
            for q in sh.pts[1:]: path.lineTo(q)
            p.drawPath(path)
            if st.arrow and len(sh.pts) > 1:
                a, b = sh.pts[-2], sh.pts[-1]
                ang = math.atan2(b.y() - a.y(), b.x() - a.x()); L = 10 + st.width * 2
                pts = [b, b - QPointF(L * math.cos(ang - .4), L * math.sin(ang - .4)),
                       b - QPointF(L * math.cos(ang + .4), L * math.sin(ang + .4))]
                p.setPen(QPen(st.color, 1)); p.setBrush(st.color); p.drawPolygon(QPolygonF(pts))
        elif sh.kind in ("pen", "marker"):
            if len(sh.pts) == 1:
                p.setPen(Qt.NoPen); p.setBrush(st.color if sh.kind == "pen" else QColor(st.color.red(), st.color.green(), st.color.blue(), 110))
                p.drawEllipse(sh.pts[0], st.width / 2, st.width / 2); continue
            if sh.kind == "marker":
                c = QColor(st.color); c.setAlpha(110); pen.setColor(c); pen.setCapStyle(Qt.FlatCap); p.setPen(pen)
                p.setCompositionMode(QPainter.CompositionMode_Multiply)
            path = QPainterPath(); path.moveTo(sh.pts[0])
            for q in sh.pts[1:]: path.lineTo(q)
            p.drawPath(path); p.setCompositionMode(QPainter.CompositionMode_SourceOver)
        elif sh.kind == "number":
            r = max(8, st.width) / 2; c = sh.pts[0]
            p.setPen(QPen(st.color.darker(120) if st.filled else st.color, 2))
            p.setBrush(st.color if st.filled else Qt.NoBrush)
            p.drawEllipse(c, r, r)
            f = QFont(); f.setBold(True); f.setPixelSize(max(8, int(st.font_size or r)))
            p.setFont(f); p.setPen(QColor("#fff") if st.color.lightness() < 140 else QColor("#111"))
            p.drawText(QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r), Qt.AlignCenter, str(sh.number or 1))
        elif sh.kind == "text" and sh.text:
            f = text_font(st); fm = QFontMetricsF(f)
            path = QPainterPath()
            for i, line in enumerate(sh.text.split("\n")):
                path.addText(sh.pts[0] + QPointF(0, fm.ascent() + i * fm.lineSpacing()), f, line)
            if st.outline:
                halo = QColor(0, 0, 0) if st.color.lightness() > 180 else QColor(255, 255, 255)
                p.setPen(QPen(halo, max(2, st.font_size / 7), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)); p.drawPath(path)
            p.setPen(Qt.NoPen); p.setBrush(st.color); p.drawPath(path)
    if selected is not None and selected in shapes:
        b = shape_bounds(selected).adjusted(-2, -2, 2, 2)
        p.setBrush(Qt.NoBrush); p.setPen(QPen(QColor(30, 144, 255), 1, Qt.DashLine)); p.drawRect(b)
        p.setBrush(QColor(30, 144, 255)); p.setPen(QPen(Qt.white, 1))
        for hx, hy in _handles(b).values():
            p.drawRect(QRectF(hx - HANDLE / 2, hy - HANDLE / 2, HANDLE, HANDLE))
    p.restore()


def _handles(b: QRectF):
    return {"tl": (b.left(), b.top()), "t": (b.center().x(), b.top()), "tr": (b.right(), b.top()),
            "l": (b.left(), b.center().y()), "r": (b.right(), b.center().y()),
            "bl": (b.left(), b.bottom()), "b": (b.center().x(), b.bottom()), "br": (b.right(), b.bottom())}


def text_font(st: Style) -> QFont:
    f = QFont(st.family) if st.family else QFont()
    f.setPixelSize(st.font_size); f.setBold(st.bold); f.setItalic(st.italic)
    return f


def snapshot_shapes(shapes):
    return deepcopy(shapes)


class Annotator(QObject):
    changed = Signal()
    toolChanged = Signal(object)

    def __init__(self, widget, base, to_img, to_widget, scale=lambda: 1.0):
        super().__init__(widget)
        self.w, self.base, self.to_img, self.to_widget, self.scale = widget, base, to_img, to_widget, scale
        self.tool = None
        self.styles = default_styles()
        self.shapes, self.redo_stack = [], []
        self.live = None
        self.editor = None; self.forwarding = False
        self.selected = None
        self.width_target = None
        self.edit_target = None
        self._wheel_target = None; self._wheel_time = 0; self._wheel_delta = 0
        self._sel_mode = None          # None | 'move' | handle name
        self._sel_origin = None
        self._sel_pts0 = None
        self._sel_dirty = False
        self._next_number = 1
        from PySide6.QtCore import QTimer
        self._poll = QTimer(self, interval=8); self._poll.timeout.connect(self._poll_cursor)
        self.cache = EffectCache()
        self._edit_undo = []           # full-document snapshots for select edits

    def set_tool(self, tool):
        self.finish()
        self.width_target = None
        self.edit_target = None; self._wheel_target = None; self._wheel_delta = 0
        if tool != "select": self.selected = None
        self.tool = tool
        self.toolChanged.emit(tool); self.changed.emit()

    def style(self):
        if self.tool == "select" or not self.tool: return None
        return self.styles.get(self.tool)

    def option_target(self):
        target = self.live or self.selected or self.edit_target or self.width_target
        return target if target is not None and target in self.all_shapes() else None

    def option_tool(self):
        target = self.option_target()
        return target.kind if target and self.tool not in ('eraser', 'mosaic') else self.tool

    def option_style(self, kind):
        target = self.option_target()
        return target.style if target and target.kind == kind else self.styles[kind]

    def set_style_value(self, kind, attr, value):
        target = self.option_target()
        if target and target.kind == kind and target in self.all_shapes():
            if getattr(target.style, attr) != value:
                if target is not self.live: self._push_edit()
                setattr(target.style, attr, value)
        setattr(self.styles[kind], attr, value)
        if self.editor: self._style_editor()

    def add(self, shape):
        self._push_edit()
        self.shapes.append(shape); self.redo_stack.clear(); self.changed.emit()
        self.width_target = shape

    def _push_edit(self):
        self._wheel_target = None
        self._edit_undo.append(snapshot_shapes(self.shapes))
        if len(self._edit_undo) > 50: del self._edit_undo[:-50]
        self.redo_stack.clear()

    def undo(self):
        self.finish()
        self.edit_target = None; self._wheel_target = None
        self.width_target = None
        if self._edit_undo:
            self.redo_stack.append(snapshot_shapes(self.shapes))
            self.shapes = self._edit_undo.pop(); self.selected = None; self._renumber(); self.changed.emit(); return
        if self.shapes:
            self.redo_stack.append(self.shapes.pop()); self.selected = None; self._renumber(); self.changed.emit()

    def redo(self):
        self.edit_target = None; self._wheel_target = None
        self.width_target = None
        if self.redo_stack:
            item = self.redo_stack.pop()
            if isinstance(item, list) and (not item or isinstance(item[0], Shape)):
                self._edit_undo.append(snapshot_shapes(self.shapes)); self.shapes = item
            else:
                self.shapes.append(item)
            self.selected = None; self._renumber(); self.changed.emit()

    def clear(self):
        self.edit_target = None; self._wheel_target = None; self._wheel_delta = 0
        self.width_target = None; self._edit_undo.clear()
        self.shapes.clear(); self.redo_stack.clear(); self.live = None; self.selected = None
        self._next_number = 1; self.changed.emit()

    def _renumber(self):
        n = 1
        for sh in self.shapes:
            if sh.kind == "number":
                sh.number = n; n += 1
        self._next_number = n

    def all_shapes(self): return self.shapes + ([self.live] if self.live else [])

    def paint(self, p: QPainter):
        render(p, self.all_shapes(), self.base(), self.cache, selected=self.selected)

    def composed(self) -> QImage:
        img = self.base().copy(); p = QPainter(img); render(p, self.shapes, self.base(), self.cache); p.end(); return img

    def adjust_width(self, delta):
        target = self.option_target()
        if target is not None and target not in self.all_shapes(): target = None
        st = target.style if target else self.style()
        if not st: return False
        kind = target.kind if target else self.tool
        now = time.monotonic()
        if target and target is not self.live:
            if target is not self._wheel_target or now - self._wheel_time > .5: self._push_edit()
            self._wheel_target = target; self._wheel_time = now
        if kind == "text": st.font_size = max(8, min(120, st.font_size + 2 * delta))
        elif kind == "mosaic": st.size = max(1, min(30, st.size + delta))
        elif kind == "number":
            st.width = max(12, min(80, st.width + 2 * delta))
            st.font_size = max(8, int(st.width * 0.65))
        else: st.width = max(1, min(60, st.width + delta))
        if target:
            for attr in ("width", "font_size", "size"):
                setattr(self.styles[kind], attr, getattr(st, attr))
        if self.editor: self._style_editor()
        self.changed.emit(); return True

    def wheel(self, angle_delta):
        self._wheel_delta += angle_delta
        steps = math.trunc(self._wheel_delta / 120)
        self._wheel_delta -= steps * 120
        return self.adjust_width(steps) if steps else False

    BRUSH_KINDS = frozenset({"pen", "marker"})

    def is_brush_tool(self, tool=None):
        tool = self.tool if tool is None else tool
        if tool in self.BRUSH_KINDS: return True
        if tool in ("mosaic", "eraser"): return self.styles[tool].mode == "brush"
        return False

    def is_brush(self, sh=None):
        sh = sh or self.live
        if sh is None: return False
        if sh.kind in self.BRUSH_KINDS: return True
        return sh.kind in ("mosaic", "eraser") and sh.style.mode == "brush"

    def cursor_pos(self):
        from PySide6.QtGui import QCursor
        return QPointF(self.w.mapFromGlobal(QCursor.pos()))

    def _append_brush(self, pt: QPointF, min_dist=0.5) -> bool:
        sh = self.live
        if sh is None or not self.is_brush(sh): return False
        if not sh.pts or (pt - sh.pts[-1]).manhattanLength() >= min_dist:
            sh.pts.append(QPointF(pt)); return True
        return False

    def sample_brush(self):
        if not self.is_brush(): return False
        if self._append_brush(self.to_img(self.cursor_pos())):
            self.changed.emit(); return True
        return False

    def hit_test(self, img_pt: QPointF):
        for sh in reversed(self.shapes):
            if sh.kind == "eraser":
                path = _stroke_path(sh.pts, sh.style.width) if sh.style.mode == "brush" else QPainterPath()
                if sh.style.mode != "brush": path.addRect(_rect(sh))
                if path.contains(img_pt): return None
                continue
            margin = max(sh.style.width / 2, sh.style.font_size / 7, sh.style.size * 3) + 2
            if sh.kind == "line" and sh.style.arrow: margin = max(margin, 12 + sh.style.width * 2)
            if not shape_bounds(sh).adjusted(-margin, -margin, margin, margin).contains(img_pt): continue
            if sh.kind == "mosaic":
                path = _stroke_path(sh.pts, sh.style.size * 6) if sh.style.mode == "brush" else QPainterPath()
                if sh.style.mode != "brush": path.addRect(_rect(sh))
                if path.contains(img_pt): return sh
                continue
            patch = QImage(3, 3, QImage.Format_ARGB32_Premultiplied); patch.fill(Qt.transparent)
            painter = QPainter(patch); painter.translate(1 - img_pt.x(), 1 - img_pt.y())
            render(painter, [sh], self.base(), self.cache); painter.end()
            if any(patch.pixelColor(x, y).alpha() > 16 for y in range(3) for x in range(3)): return sh
        return None

    def hover(self, pos):
        if self.live or self._sel_mode or self.editor: return
        pt = self.to_img(pos)
        hit = None if self.tool in ("eraser", "mosaic") else (self.selected if self.hit_handle(pt) else self.hit_test(pt))
        if hit is not self.selected:
            self.selected = hit
            if hit: self.edit_target = hit
            self.changed.emit()

    def hit_handle(self, img_pt: QPointF):
        if not self.selected: return None
        b = shape_bounds(self.selected).adjusted(-2, -2, 2, 2)
        for name, (hx, hy) in _handles(b).items():
            if abs(img_pt.x() - hx) <= HANDLE + 2 and abs(img_pt.y() - hy) <= HANDLE + 2:
                return name
        return None

    def delete_selected(self):
        if self.selected and self.selected in self.shapes:
            self._push_edit()
            self.shapes.remove(self.selected); self.selected = None
            self._renumber(); self.changed.emit(); return True
        return False

    def press(self, pos, button, mods=Qt.NoModifier):
        if button == Qt.RightButton:
            if not self.tool: return False
            self.finish(); return True
        if button != Qt.LeftButton: return False
        pt = self.to_img(pos)

        if self.tool == "select" or (self.tool not in ("eraser", "mosaic") and (self.hit_test(pt) or self.hit_handle(pt))):
            h = self.hit_handle(pt)
            if h and self.selected:
                self._sel_mode = h; self._sel_origin = pt; self._sel_dirty = False
                self._sel_pts0 = [QPointF(p) for p in self.selected.pts]; return True
            hit = self.hit_test(pt)
            self.selected = hit
            if hit:
                self._sel_mode = "move"; self._sel_origin = pt; self._sel_dirty = False
                self._sel_pts0 = [QPointF(p) for p in hit.pts]
            self.changed.emit(); return True

        if not self.tool: return False
        self.width_target = None; self.edit_target = None

        st = copy(self.style())
        if self.tool == "text":
            if self.editor: self.commit_text(); return True
            self.start_text(pt); return True
        if self.tool == "number":
            sh = Shape("number", [QPointF(pt)], st, number=self._next_number)
            self._next_number += 1; self.add(sh); return True
        if self.tool == "line" and (mods & Qt.ShiftModifier) and self.shapes and self.shapes[-1].kind == "line":
            self.live = self.shapes.pop(); self.live.pts.append(pt); return True
        brush = self.is_brush_tool()
        self.live = Shape(self.tool, [QPointF(pt)], st) if brush else Shape(self.tool, [QPointF(pt), QPointF(pt)], st)
        if brush: self._poll.start()
        self.changed.emit(); return True

    def _poll_cursor(self):
        if not self.is_brush():
            self._poll.stop(); return
        self.sample_brush()

    def move(self, pos, mods=Qt.NoModifier):
        pt = self.to_img(pos)
        if self.selected and self._sel_mode and self._sel_pts0 is not None:
            if pt == self._sel_origin: return True
            if not getattr(self, "_sel_dirty", False):
                self._push_edit(); self._sel_dirty = True
            sh = self.selected
            if self._sel_mode == "move":
                d = pt - self._sel_origin
                sh.pts = [QPointF(p) + d for p in self._sel_pts0]
            else:
                self._resize_selected(pt)
            self.changed.emit(); return True
        if not self.live: return False
        sh = self.live
        if self.is_brush(sh):
            changed = self._append_brush(self.to_img(self.cursor_pos()))
            changed = self._append_brush(pt) or changed
            if changed: self.changed.emit()
            return True
        if (mods & Qt.ShiftModifier) and sh.kind in ("rect", "ellipse"):
            a = sh.pts[0]; d = max(abs(pt.x() - a.x()), abs(pt.y() - a.y()))
            pt = QPointF(a.x() + math.copysign(d, pt.x() - a.x()), a.y() + math.copysign(d, pt.y() - a.y()))
        sh.pts[-1] = pt
        self.changed.emit(); return True

    def _resize_selected(self, pt: QPointF):
        sh = self.selected; h = self._sel_mode
        if sh.kind in ("number",) and len(sh.pts) == 1:
            # resize diameter from center
            c = self._sel_pts0[0]; r = max(8, (pt - c).manhattanLength())
            sh.style.width = int(r * 2); sh.style.font_size = max(8, int(r * 1.2)); return
        if sh.kind in ("pen", "marker", "line") and len(self._sel_pts0) > 2:
            # uniform scale about bounds center
            b0 = QRectF(self._sel_pts0[0], self._sel_pts0[-1])
            xs = [p.x() for p in self._sel_pts0]; ys = [p.y() for p in self._sel_pts0]
            b0 = QRectF(QPointF(min(xs), min(ys)), QPointF(max(xs), max(ys)))
            c = b0.center(); dx0 = max(1, abs(b0.width()) / 2); dy0 = max(1, abs(b0.height()) / 2)
            sx = abs(pt.x() - c.x()) / dx0; sy = abs(pt.y() - c.y()) / dy0
            s = max(0.05, (sx + sy) / 2)
            sh.pts = [c + (p - c) * s for p in self._sel_pts0]; return
        # rect/ellipse/mosaic/text/2-pt shapes: adjust corners
        b = QRectF(self._sel_pts0[0], self._sel_pts0[-1]).normalized()
        if "l" in h: b.setLeft(pt.x())
        if "r" in h: b.setRight(pt.x())
        if h.startswith("t") or h == "t": b.setTop(pt.y())
        if h.startswith("b") or h == "b": b.setBottom(pt.y())
        b = b.normalized()
        if sh.kind == "text":
            sh.pts = [b.topLeft()]
        else:
            sh.pts = [b.topLeft(), b.bottomRight()]

    def release(self, pos):
        if self._sel_mode or self.tool == "select":
            self._sel_mode = None; self._sel_origin = None; self._sel_pts0 = None
            self.changed.emit(); return True
        if not self.live: return False
        if self.is_brush():
            self._append_brush(self.to_img(self.cursor_pos()), min_dist=0)
            self._append_brush(self.to_img(pos), min_dist=0)
        self.finish(); return True

    def finish(self):
        if self.editor: self.commit_text()
        self._poll.stop()
        sh, self.live = self.live, None
        if sh is None: return
        a, b = sh.pts[0], sh.pts[-1]
        ok = len(sh.pts) > 1 and ((b - a).manhattanLength() > 2 or len(sh.pts) > 2)
        if ok: self.add(sh)
        self.changed.emit()

    def start_text(self, pt, existing=None):
        ed = QLineEdit(self.w); ed.img_pt = pt; self.editor = ed
        ed.setFrame(False)
        if existing is not None:
            ed._edit_shape = existing; ed.setText(existing.text)
        self._style_editor()
        wp = self.to_widget(pt); ed.move(int(wp.x()), int(wp.y()))
        ed.returnPressed.connect(self.commit_text)
        ed.textChanged.connect(lambda *_: ed.resize(max(80, ed.fontMetrics().horizontalAdvance(ed.text() + "MM")), ed.height()))
        ed.show(); ed.raise_(); self.w.activateWindow(); ed.setFocus(Qt.MouseFocusReason)
        self.changed.emit()

    def begin_edit_text(self, sh):
        if sh.kind != "text": return
        self.set_tool("select"); self.selected = sh
        self.start_text(sh.pts[0], existing=sh)

    def _style_editor(self):
        ed = self.editor
        st = ed._edit_shape.style if getattr(ed, "_edit_shape", None) else self.styles["text"]
        f = text_font(st); f.setPixelSize(max(8, int(st.font_size * self.scale()))); ed.setFont(f)
        ed.setStyleSheet("QLineEdit{background:rgba(255,255,255,90);color:%s;border:1px dashed %s;padding:0}"
                         % (st.color.name(), st.color.name()))
        ed.resize(max(80, ed.fontMetrics().horizontalAdvance(ed.text() + "MM")), ed.fontMetrics().height() + 4)

    def commit_text(self, cancel=False):
        ed, self.editor = self.editor, None
        if ed is None: return
        existing = getattr(ed, "_edit_shape", None)
        if not cancel and ed.text().strip():
            if existing is not None:
                self._push_edit(); existing.text = ed.text()
            else:
                self.add(Shape("text", [ed.img_pt], copy(self.styles["text"]), ed.text()))
        ed.hide(); ed.deleteLater(); self.w.setFocus(); self.changed.emit()

    def key(self, e) -> bool:
        if self.editor:
            if e.key() == Qt.Key_Escape: self.commit_text(cancel=True); return True
            from PySide6.QtWidgets import QApplication
            self.forwarding = True
            try: QApplication.sendEvent(self.editor, e)
            finally: self.forwarding = False
            return True
        if self.tool == "select" and e.key() in (Qt.Key_Delete, Qt.Key_Backspace):
            return self.delete_selected()
        return False
