# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for PinShot (Windows --onedir --windowed). Build: pyinstaller --noconfirm pinshot.spec
# The icon comes from tools/make_icon.py (our own QPainter drawing).
import os, sys
from PyInstaller.utils.hooks import collect_submodules, collect_data_files

ROOT = os.path.abspath(SPECPATH)
ICON = os.path.join(ROOT, "assets", "pinshot.ico")

hidden = collect_submodules("pinshot") + ["comtypes", "comtypes.client", "comtypes.stream", "rapidocr.main", "onnxruntime"]
excludes = ["Xlib", "pynput", "gi", "tkinter",
            "PySide6.QtWebEngineCore", "PySide6.QtWebEngineWidgets", "PySide6.Qt3DCore", "PySide6.QtQuick3D",
            "PySide6.QtMultimedia", "PySide6.QtCharts", "PySide6.QtDataVisualization", "PySide6.QtPdf"]

a = Analysis(
    [os.path.join(ROOT, "pinshot_launcher.py")],
    pathex=[ROOT],
    binaries=[],
    datas=[(os.path.join(ROOT, "assets", "pinshot.png"), "assets")]
          + collect_data_files('rapidocr', excludes=['models/*.onnx'])
          + [(os.path.join(ROOT, 'assets', 'ocr-models'), 'assets/ocr-models')],
    hiddenimports=hidden,
    hookspath=[],
    runtime_hooks=[],
    excludes=excludes,
    noarchive=False,
)
pyz = PYZ(a.pure)

# Qt 6.12 uses Windows' ICU exports. A versioned third-party ICU found on
# PATH has incompatible symbol names and must not shadow the system DLL.
if sys.platform == "win32":
    a.binaries = [item for item in a.binaries
                  if not os.path.basename(item[0]).lower().startswith("icu")]

exe = EXE(
    pyz, a.scripts, [],
    exclude_binaries=True,          # --onedir
    name="PinShot",
    debug=False,
    strip=False,
    upx=False,
    console=False,                  # --windowed
    icon=ICON if os.path.exists(ICON) else None,
    version=None,
)
coll = COLLECT(exe, a.binaries, a.datas, strip=False, upx=False, name="PinShot")
