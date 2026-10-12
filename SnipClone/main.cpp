//==============================================================================
// SnipClone — entry point & state machine (V0)
//
// States implemented:
//   Idle  --Ctrl+Shift+S-->  SelectionOverlay  --confirm/ESC-->  Idle
//
// V1+: Selection confirmed → ScreenCapture → AnnotationOverlay → Pin/Copy/Save
//==============================================================================
#include "Hotkey/GlobalHotkey.h"
#include "SelectionOverlay/SelectionOverlay.h"

#include <Windows.h>

#include <memory>

namespace
{
constexpr wchar_t kMainClass[] = L"SnipCloneMain";
constexpr int kHotkeySnip = 0x1001;

HINSTANCE g_instance = nullptr;
HWND g_mainWnd = nullptr;
std::unique_ptr<snip::SelectionOverlay> g_overlay;

void DebugLogSelection(bool confirmed)
{
    const auto& sel = snip::SelectionOverlay::LastSelection();
    wchar_t buf[256]{};
    if (confirmed && sel.hasSelection)
    {
        swprintf_s(
            buf,
            L"[SnipClone V0] selection confirmed: (%ld,%ld)-(%ld,%ld) %ldx%ld\n",
            sel.screenRect.left,
            sel.screenRect.top,
            sel.screenRect.right,
            sel.screenRect.bottom,
            sel.Width(),
            sel.Height());
    }
    else
    {
        swprintf_s(buf, L"[SnipClone V0] selection cancelled\n");
    }
    OutputDebugStringW(buf);
}

void BeginSelection()
{
    if (g_overlay && g_overlay->IsActive())
    {
        return;
    }

    g_overlay = std::make_unique<snip::SelectionOverlay>();
    if (!g_overlay->Start(g_instance, g_mainWnd))
    {
        g_overlay.reset();
        MessageBoxW(
            g_mainWnd,
            L"Failed to create selection overlay.",
            L"SnipClone",
            MB_ICONERROR | MB_OK);
    }
}

LRESULT CALLBACK MainWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg)
    {
    case WM_HOTKEY:
        if (static_cast<int>(wParam) == kHotkeySnip)
        {
            BeginSelection();
        }
        return 0;

    case snip::WM_SNIP_SELECTION_DONE:
        DebugLogSelection(wParam != 0);
        // Keep LastSelection for V1 capture; tear down overlay instance.
        if (g_overlay && !g_overlay->IsActive())
        {
            g_overlay.reset();
        }
        return 0;

    case WM_DESTROY:
        if (g_overlay)
        {
            if (g_overlay->IsActive())
            {
                g_overlay->Stop(false);
            }
            g_overlay.reset();
        }
        PostQuitMessage(0);
        return 0;

    default:
        return DefWindowProcW(hwnd, msg, wParam, lParam);
    }
}

bool RegisterMainClass(HINSTANCE instance)
{
    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = MainWndProc;
    wc.hInstance = instance;
    wc.hIcon = LoadIconW(nullptr, IDI_APPLICATION);
    wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    wc.lpszClassName = kMainClass;
    return RegisterClassExW(&wc) != 0;
}
} // namespace

int WINAPI wWinMain(
    HINSTANCE hInstance,
    HINSTANCE /*hPrevInstance*/,
    PWSTR /*lpCmdLine*/,
    int /*nShowCmd*/)
{
    g_instance = hInstance;

    // Prefer Per-Monitor V2 when available (manifest also declares it).
    using SetDpiAwFn = BOOL(WINAPI*)(DPI_AWARENESS_CONTEXT);
    if (auto setDpi = reinterpret_cast<SetDpiAwFn>(
            GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext")))
    {
        setDpi(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    if (!RegisterMainClass(hInstance))
    {
        return 1;
    }

    // Message-only style hidden owner for hotkeys (0-size popup).
    g_mainWnd = CreateWindowExW(
        0,
        kMainClass,
        L"SnipClone",
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        nullptr,
        nullptr,
        hInstance,
        nullptr);
    if (!g_mainWnd)
    {
        return 1;
    }

    // Ctrl+Shift+S — avoid conflicting with browsers' Ctrl+S.
    if (!snip::GlobalHotkey::Register(VK_S, MOD_CONTROL | MOD_SHIFT, kHotkeySnip, g_mainWnd))
    {
        MessageBoxW(
            nullptr,
            L"Failed to register hotkey Ctrl+Shift+S.\n"
            L"Another app may already own it.",
            L"SnipClone",
            MB_ICONWARNING | MB_OK);
    }

    MSG msg{};
    while (GetMessageW(&msg, nullptr, 0, 0) > 0)
    {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    snip::GlobalHotkey::Unregister(kHotkeySnip, g_mainWnd);
    return static_cast<int>(msg.wParam);
}
