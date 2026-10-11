"""Force a window to the top of the X stack: EWMH _NET_WM_STATE_ABOVE + _NET_ACTIVE_WINDOW
(+ _NET_RESTACK_WINDOW) client messages that the WM honours, plus XRaiseWindow/SetInputFocus."""


def _client_msg(d, root, win, atom, data):
    from Xlib import X
    from Xlib.protocol import event
    ev = event.ClientMessage(window=win, client_type=d.intern_atom(atom), data=(32, (data + [0] * 5)[:5]))
    root.send_event(ev, event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask)


def set_skip_taskbar(qt_widget) -> bool:
    """Hide a (non-Tool) window from taskbar/pager via EWMH, as Qt.Tool would."""
    try:
        from Xlib import display
        d = display.Display(); root = d.screen().root
        try:
            win = d.create_resource_object('window', int(qt_widget.winId()))
            _client_msg(d, root, win, '_NET_WM_STATE', [1, d.intern_atom('_NET_WM_STATE_SKIP_TASKBAR'),
                                                        d.intern_atom('_NET_WM_STATE_SKIP_PAGER'), 2])
            d.sync()
        finally:
            d.close()
        return True
    except Exception:
        return False


def raise_on_top(qt_widget, lower=()) -> bool:
    """Raise qt_widget above everything; push widgets in `lower` to the bottom of the ABOVE layer."""
    try:
        from Xlib import display, X
        d = display.Display(); root = d.screen().root
        try:
            for w in lower:
                try:
                    xw = d.create_resource_object('window', int(w.winId()))
                    _client_msg(d, root, xw, '_NET_RESTACK_WINDOW', [2, 0, X.Below])
                    xw.configure(stack_mode=X.Below)
                except Exception:
                    pass
            win = d.create_resource_object('window', int(qt_widget.winId()))
            _client_msg(d, root, win, '_NET_WM_STATE', [1, d.intern_atom('_NET_WM_STATE_ABOVE'), 0, 2])
            _client_msg(d, root, win, '_NET_RESTACK_WINDOW', [2, 0, X.Above])
            _client_msg(d, root, win, '_NET_ACTIVE_WINDOW', [2, X.CurrentTime, 0])
            win.configure(stack_mode=X.Above)
            d.sync()
        finally:
            d.close()
        return True
    except Exception:
        return False
