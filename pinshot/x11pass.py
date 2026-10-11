"""X11 input pass-through: clear ShapeInput on the client window and its WM frame."""
from __future__ import annotations


def set_input_passthrough(qt_widget, on: bool) -> bool:
    """Make (or restore) an empty X11 input shape so clicks fall through.
    Applies to winId() and, if reparented, the immediate WM frame parent.
    Returns True if XShape was applied."""
    try:
        from Xlib import display, X
        from Xlib.ext import shape
    except Exception:
        return False
    try:
        wid = int(qt_widget.winId())
    except Exception:
        return False
    if not wid:
        return False
    d = display.Display()
    try:
        if not d.has_extension("SHAPE"):
            return False
        targets = []
        w = d.create_resource_object("window", wid)
        targets.append(w)
        # Walk up to the WM frame (reparented under root's child)
        try:
            tree = w.query_tree()
            parent = tree.parent
            root = d.screen().root
            if parent and parent.id != root.id:
                targets.append(parent)
                # sometimes one more level (decorator)
                try:
                    gp = parent.query_tree().parent
                    if gp and gp.id != root.id:
                        targets.append(gp)
                except Exception:
                    pass
        except Exception:
            pass
        for win in targets:
            if on:
                # Empty input region → events pass through to whatever is below.
                win.shape_rectangles(shape.SO.Set, shape.SK.Input, 0, 0, 0, [])
            else:
                # Restore input to match the bounding box of the window.
                try:
                    g = win.get_geometry()
                    win.shape_rectangles(shape.SO.Set, shape.SK.Input, 0, 0, 0,
                                         [(0, 0, max(1, g.width), max(1, g.height))])
                except Exception:
                    # Fallback: copy bounding shape into input shape
                    win.shape_mask(shape.SO.Set, shape.SK.Input, 0, 0, X.NONE)
        d.sync()
        return True
    except Exception as e:
        print("[pinshot] x11pass:", e, flush=True)
        return False
    finally:
        try: d.close()
        except Exception: pass
