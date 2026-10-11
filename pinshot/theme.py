"""Light / Dark / Follow-system UI theme. Chrome only: annotation colors and the accent
(selection blue, handles, pin glow) are painted explicitly and are not touched."""
from PySide6.QtCore import Qt, QObject, Signal
from PySide6.QtGui import QPalette, QColor, QGuiApplication
from PySide6.QtWidgets import QApplication, QStyleFactory

MODES = [("system", "Follow system"), ("light", "Light"), ("dark", "Dark")]
ACCENT = "#1e90ff"

TOOLBAR = {
    "light": dict(bg="#f7f7f7", border="#9a9a9a", hover="#dde8f5", checked="#c4d9f2", sep="#c8c8c8", fg="#333333",
                  field="#ffffff", confirm="#fafafa"),
    "dark": dict(bg="#2b2d31", border="#5a5d63", hover="#3a4a60", checked="#2f5687", sep="#55585e", fg="#e6e6e6",
                 field="#1f2023", confirm="#2b2d31"),
}

_effective = "light"
_mode = "system"


def system_is_dark():
    try:
        return QGuiApplication.styleHints().colorScheme() == Qt.ColorScheme.Dark
    except Exception:
        return False


def effective(mode=None):
    mode = mode or _mode
    if mode == "system": return "dark" if system_is_dark() else "light"
    return "dark" if mode == "dark" else "light"


def current(): return _effective


def icon_color():
    return QColor(228, 228, 228) if _effective == "dark" else QColor(50, 50, 50)


def toolbar_css(eff=None):
    c = TOOLBAR[eff or _effective]
    return f"""
QFrame#bar, QFrame#panel {{ background:{c['bg']}; border:1px solid {c['border']}; border-radius:3px; }}
QFrame#bar QToolButton, QFrame#panel QToolButton {{ border:none; padding:3px; border-radius:3px; background:transparent; color:{c['fg']}; }}
QFrame#bar QToolButton:hover, QFrame#panel QToolButton:hover {{ background:{c['hover']}; }}
QFrame#bar QToolButton:checked, QFrame#panel QToolButton:checked {{ background:{c['checked']}; }}
QFrame#sep {{ background:{c['sep']}; }}
QFrame#panel QLabel {{ color:{c['fg']}; font-size:11px; }}
QFrame#panel QSpinBox, QFrame#panel QComboBox, QFrame#panel QFontComboBox {{ background:{c['field']}; color:{c['fg']};
    border:1px solid {c['border']}; border-radius:2px; padding:1px 3px; }}
"""


def confirm_css(eff=None):
    c = TOOLBAR[eff or _effective]
    return (f"QFrame#confirm{{background:{c['confirm']};border:1px solid {c['border']};border-radius:4px}}"
            f"QFrame#confirm QLabel, QFrame#confirm QCheckBox{{font-size:13px;color:{c['fg']}}}")


def _dark_palette():
    p = QPalette()
    win, base, text = QColor(43, 45, 49), QColor(31, 32, 35), QColor(230, 230, 230)
    for role, col in [(QPalette.Window, win), (QPalette.WindowText, text), (QPalette.Base, base),
                      (QPalette.AlternateBase, QColor(50, 52, 57)), (QPalette.ToolTipBase, QColor(50, 52, 57)),
                      (QPalette.ToolTipText, text), (QPalette.Text, text), (QPalette.Button, QColor(53, 55, 60)),
                      (QPalette.ButtonText, text), (QPalette.BrightText, QColor(255, 80, 80)),
                      (QPalette.Highlight, QColor(ACCENT)), (QPalette.HighlightedText, QColor(255, 255, 255)),
                      (QPalette.Link, QColor(ACCENT)), (QPalette.PlaceholderText, QColor(140, 140, 140))]:
        p.setColor(role, col)
    for role in (QPalette.WindowText, QPalette.Text, QPalette.ButtonText):
        p.setColor(QPalette.Disabled, role, QColor(120, 120, 120))
    return p


class ThemeBus(QObject):
    changed = Signal(str)


BUS = ThemeBus()


def apply(mode="system"):
    """Apply app-wide; restyles live widgets (toolbars, panels, confirm boxes) in place."""
    global _mode, _effective
    app = QApplication.instance()
    _mode = mode if mode in dict(MODES) else "system"
    _effective = effective(_mode)
    app.setStyle(QStyleFactory.create("Fusion"))
    pal = _dark_palette() if _effective == "dark" else app.style().standardPalette()
    if _effective == "light": pal.setColor(QPalette.Highlight, QColor(ACCENT))
    app.setPalette(pal)
    for w in QApplication.allWidgets():
        try:
            kind = w.property("pinshot_css")
            if kind == "toolbar": w.setStyleSheet(toolbar_css())
            elif kind == "confirm": w.setStyleSheet(confirm_css())
            ic = w.property("pinshot_icon")
            if ic:
                from .icons import icon
                w.setIcon(icon(ic, icon_color()))
            if w.isWindow(): w.update()
        except RuntimeError:
            pass
    BUS.changed.emit(_effective)
    return _effective


def watch_system():
    """Re-apply when the desktop flips light/dark while in Follow-system mode."""
    try:
        QGuiApplication.styleHints().colorSchemeChanged.connect(lambda *_: _mode == "system" and apply("system"))
    except Exception:
        pass
