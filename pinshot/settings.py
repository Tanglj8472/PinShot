"""PinShot preferences (QSettings)."""
from PySide6.QtCore import QSettings, QStandardPaths

ORG, APP = "PinShot", "PinShot"

DEFAULTS = {
    "run_at_login": False,
    "start_minimized": False,
    "skip_abort_confirm": False,
    "dblclick_pin_action": "close",  # close | hide | none
    "dblclick_tip_shown": False,
    "hotkey_snip": "F1",
    "hotkey_pin": "F3",
    "hotkey_hide_all": "Shift+F3",
    "hotkey_clickthrough": "",
    "hotkey_restore": "Ctrl+Shift+Z",       # unbound by default
    "quick_save_folder": "",
    "image_quality": 92,
    "snip_delay": 0,                 # seconds before snip (0/1/2/3/5)
             # JPEG quality 1-100
    "language": "en",                # en | zh_CN
    "theme": "system",               # system | light | dark
    "history_max": 10,               # snip history with images
    "closed_pins_max": 10,           # recoverable Close stack
    "groups": ["Default", "Group 1", "Group 2", "Group 3"],
}


def settings() -> QSettings:
    return QSettings(ORG, APP)


def get(key, typ=None):
    s = settings()
    default = DEFAULTS.get(key)
    if typ is None:
        typ = type(default) if default is not None else str
    if key == "groups":
        v = s.value(key)
        if isinstance(v, list) and v: return [str(x) for x in v]
        if isinstance(v, str) and v: return [x for x in v.split("\n") if x]
        return list(DEFAULTS["groups"])
    if key == "quick_save_folder":
        v = s.value(key, "")
        return v or QStandardPaths.writableLocation(QStandardPaths.PicturesLocation)
    if typ is bool: return s.value(key, default, type=bool)
    if typ is int: return int(s.value(key, default))
    return s.value(key, default)


def set_(key, value):
    if key == "groups" and isinstance(value, list):
        seen, out = set(), []
        for n in value:
            n = str(n).strip()
            if n and n.lower() not in seen: seen.add(n.lower()); out.append(n)
        value = out or ["Default"]
    settings().setValue(key, value)


def reset_key(key):
    settings().remove(key)
