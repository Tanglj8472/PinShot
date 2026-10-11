"""Platform-layer unit tests. Run on Linux and Windows (no display needed): python tests/test_platform.py
On Linux the Windows backend's pure logic is checked with a fake user32/dwmapi."""
import os, sys, unittest, importlib
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from pinshot.platform import windows as W
from pinshot.platform import base as B
import pinshot.platform as PL


class Hotkeys(unittest.TestCase):
    def test_parse(self):
        self.assertEqual(W.parse_hotkey("F1"), (0, 0x70))
        self.assertEqual(W.parse_hotkey("Shift+F3"), (W.MOD_SHIFT, 0x72))
        self.assertEqual(W.parse_hotkey("Ctrl+Shift+Z"), (W.MOD_CONTROL | W.MOD_SHIFT, ord("Z")))
        self.assertEqual(W.parse_hotkey("ctrl+alt+t"), (W.MOD_CONTROL | W.MOD_ALT, ord("T")))
        self.assertEqual(W.parse_hotkey("Win+PrintScreen"), (W.MOD_WIN, 0x2C))
        self.assertEqual(W.parse_hotkey("F24"), (0, 0x87))
        for bad in ("", None, "Hyper+X", "Ctrl+", "F25", "Ctrl+Foo"):
            self.assertIsNone(W.parse_hotkey(bad), bad)


class Mapping(unittest.TestCase):
    def test_single_scaled_screen(self):
        self.assertEqual(W.phys_to_logical(300, 150, 600, 300, [(0, 0, 1280, 720, 1.5)]), (200, 100, 400, 200))

    def test_mixed_dpi(self):
        screens = [(0, 0, 1280, 720, 1.5), (1920, 0, 1920, 1080, 1.0)]
        self.assertEqual(W.phys_to_logical(2000, 100, 400, 300, screens), (2000, 100, 400, 300))
        self.assertEqual(W.phys_to_logical(0, 0, 1920, 1080, screens), (0, 0, 1280, 720))

    def test_negative_origin_monitor(self):
        screens = [(0, 0, 1920, 1080, 1.0), (-1280, 0, 1280, 1024, 1.25)]
        self.assertEqual(W.phys_to_logical(-1280, 0, 1600, 1280, screens), (-1280, 0, 1280, 1024))


class Autostart(unittest.TestCase):
    def test_run_key_command(self):
        self.assertEqual(W.run_key_command([r"C:\Py\python.exe", "-m", "pinshot"]), r"C:\Py\pythonw.exe -m pinshot")
        self.assertEqual(W.run_key_command([r"C:\Program Files\PinShot\PinShot.exe"]), r'"C:\Program Files\PinShot\PinShot.exe"')

    def test_launch_command(self):
        cmd = B.BasePlatform.launch_command()
        self.assertEqual(cmd[1:], ["-m", "pinshot"]) if not getattr(sys, "frozen", False) else self.assertEqual(len(cmd), 1)


class Selection(unittest.TestCase):
    def test_current(self):
        PL._CURRENT = None
        p = PL.current()
        exp = "windows" if sys.platform == "win32" else ("linux-x11" if sys.platform.startswith("linux") else "generic")
        self.assertEqual(p.name, exp)

    def test_base_noops(self):
        b = B.BasePlatform()
        self.assertEqual(b.window_rects(), []); self.assertEqual(b.element_rects(), [])
        self.assertEqual(b.install_hotkeys(None, {}), (None, None))
        self.assertFalse(b.set_clickthrough(None, True)); self.assertFalse(b.exclude_from_capture(None))
        self.assertIsNone(b.clipboard_read()); self.assertTrue(b.instance_name().startswith("pinshot-"))

    def test_ipc_name_without_getuid(self):
        """ipc must import on Windows (no os.getuid)."""
        import pinshot.ipc as ipc
        real = getattr(os, "getuid", None)
        try:
            if real: del os.getuid
            importlib.reload(ipc)
            self.assertTrue(ipc.SERVER_NAME.startswith("pinshot-"))
        finally:
            if real: os.getuid = real
            importlib.reload(ipc)


class FakeUser32:
    """Just enough of user32 for set_clickthrough's bit logic."""
    def __init__(self, ex=0):
        self.ex = ex; self.alpha = None
        outer = self
        class F:
            def __init__(self, fn): self.fn = fn; self.restype = None; self.argtypes = None
            def __call__(self, *a): return self.fn(*a)
        self.GetWindowLongPtrW = F(lambda h, i: outer.ex)
        self.SetWindowLongPtrW = F(lambda h, i, v: setattr(outer, "ex", v) or 1)
        self.SetLayeredWindowAttributes = F(lambda h, k, a, f: setattr(outer, "alpha", a) or 1)


class FakeWidget:
    def __init__(self, op=0.5): self.op = op
    def winId(self): return 0x1234
    def windowOpacity(self): return self.op


@unittest.skipIf(sys.platform == "win32", "uses a fake user32; real path covered by win_smoke.py")
class ClickThroughLogic(unittest.TestCase):
    def setUp(self):
        self.real = W._u32
    def tearDown(self):
        W._u32 = self.real

    def test_on_off(self):
        fake = FakeUser32(ex=0); W._u32 = lambda: fake
        P = W.WindowsPlatform()
        self.assertTrue(P.set_clickthrough(FakeWidget(0.5), True))
        self.assertTrue(fake.ex & W.WS_EX_TRANSPARENT and fake.ex & W.WS_EX_LAYERED)
        self.assertEqual(fake.alpha, 128)                       # fresh layered window gets an alpha
        self.assertTrue(P.set_clickthrough(FakeWidget(), False))
        self.assertFalse(fake.ex & W.WS_EX_TRANSPARENT)
        self.assertTrue(fake.ex & W.WS_EX_LAYERED)              # Qt keeps owning WS_EX_LAYERED

    def test_already_layered_keeps_alpha(self):
        fake = FakeUser32(ex=W.WS_EX_LAYERED); W._u32 = lambda: fake
        W.WindowsPlatform().set_clickthrough(FakeWidget(0.3), True)
        self.assertIsNone(fake.alpha)


if __name__ == "__main__":
    r = unittest.main(exit=False, verbosity=2).result
    sys.exit(0 if r.wasSuccessful() else 1)
