"""Fresh X11 CLIPBOARD reader (python-xlib), bypassing Qt's clipboard cache. Supports INCR transfers."""
import time

PREFERRED = ["image/png", "image/jpeg", "image/bmp", "image/gif", "image/webp"]


def read(timeout=2.0, log=print, prefer=None):
    """Return (owner_is_us_unknown, {target: bytes}) for the best image target, or None."""
    try:
        from Xlib import display, X, Xatom
    except Exception:
        return None
    d = display.Display()
    try:
        w = d.screen().root.create_window(0, 0, 1, 1, 0, X.CopyFromParent, event_mask=X.PropertyChangeMask)
        CLIP, PROP, INCR = d.intern_atom("CLIPBOARD"), d.intern_atom("PINSHOT_SEL"), d.intern_atom("INCR")
        owner = d.get_selection_owner(CLIP)
        if owner == X.NONE:
            log("x11clip: no CLIPBOARD owner"); return None

        def convert(target):
            w.convert_selection(CLIP, target, PROP, X.CurrentTime); d.flush()
            end = time.time() + timeout
            while time.time() < end:
                if d.pending_events():
                    ev = d.next_event()
                    if ev.type == X.SelectionNotify and ev.selection == CLIP:
                        if ev.property == X.NONE: return None
                        p = w.get_full_property(PROP, X.AnyPropertyType)
                        if p is None: return None
                        if p.property_type == INCR:
                            return _incr(d, w, PROP, end)
                        w.delete_property(PROP); d.flush()
                        return p
                else:
                    time.sleep(0.005)
            return None

        def _incr(d, w, PROP, end):
            w.delete_property(PROP); d.flush(); buf = bytearray(); ptype = None
            while time.time() < end + 5:
                if not d.pending_events(): time.sleep(0.005); continue
                ev = d.next_event()
                if ev.type == X.PropertyNotify and ev.atom == PROP and ev.state == X.PropertyNewValue:
                    p = w.get_full_property(PROP, X.AnyPropertyType)
                    w.delete_property(PROP); d.flush()
                    if p is None: continue
                    chunk = p.value if isinstance(p.value, (bytes, bytearray)) else bytes(p.value)
                    if not chunk:
                        return type("P", (), {"value": bytes(buf), "property_type": ptype})()
                    ptype = p.property_type; buf += chunk
            return None

        t = convert(d.intern_atom("TARGETS"))
        names = [d.get_atom_name(a) for a in (t.value if t else [])]
        log(f"x11clip: owner=0x{owner.id:x} targets={names}")
        order = prefer if prefer is not None else PREFERRED + [n for n in names if n.startswith("image/")]
        for name in order:
            if name in names:
                p = convert(d.intern_atom(name))
                if p is not None and p.value:
                    data = p.value if isinstance(p.value, (bytes, bytearray)) else bytes(p.value)
                    log(f"x11clip: read {name} {len(data)} bytes"); return name, bytes(data)
        return None
    except Exception as e:
        log(f"x11clip: error {e!r}"); return None
    finally:
        d.close()
