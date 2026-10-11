"""Run-at-login via ~/.config/autostart/pinshot.desktop (XDG Autostart)."""
import os, sys
from pathlib import Path

DESKTOP_NAME = "pinshot.desktop"


def autostart_path() -> Path:
    return Path.home() / ".config" / "autostart" / DESKTOP_NAME


def run_script() -> str:
    """Absolute path to the PinShot launcher."""
    root = Path(__file__).resolve().parent.parent
    sh = root / "run.sh"
    if sh.is_file():
        return str(sh)
    py = root / ".venv" / "bin" / "python"
    if py.is_file():
        return f"{py} -m pinshot"
    return f"{sys.executable} -m pinshot"


def is_enabled() -> bool:
    return autostart_path().is_file()


def set_enabled(on: bool) -> Path:
    path = autostart_path()
    if on:
        path.parent.mkdir(parents=True, exist_ok=True)
        exec_ = run_script()
        body = "\n".join([
            "[Desktop Entry]",
            "Type=Application",
            "Version=1.0",
            "Name=PinShot",
            "Comment=Screenshot and pin tool",
            f"Exec={exec_}",
            "Terminal=false",
            "Categories=Utility;Graphics;",
            "X-GNOME-Autostart-enabled=true",
            "",
        ])
        path.write_text(body)
    elif path.exists():
        path.unlink()
    return path
