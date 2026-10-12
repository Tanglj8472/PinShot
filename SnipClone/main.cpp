//==============================================================================
// SnipClone — entry / state machine
//
// 装配对齐 PinShot App.snip：
//   Idle → Capture(virtual freeze) → Selection(on freeze) → crop → Annotation
//        → copy / save / pin → Idle
//
// 底层：ZoomIt 风格 GDI BitBlt|CAPTUREBLT + D2D 矢量批注 + WIC/剪贴板
// UX：PinShot 工具栏动作与 Annotator 所有权拆分
//==============================================================================
#include "AnnotationOverlay/AnnotationOverlay.h"
#include "Capture/ScreenCapture.h"
#include "Hotkey/GlobalHotkey.h"
#include "PinWindow/PinManager.h"
#include "SelectionOverlay/SelectionOverlay.h"

#include "Common/WinCompat.h"

#include <memory>

namespace
{
constexpr wchar_t kMainClass[] = L"SnipCloneMain";
constexpr int kHotkeySnip = 0x1001;

HINSTANCE g_instance = nullptr;
HWND g_mainWnd = nullptr;
std::unique_ptr<snip::SelectionOverlay> g_selection;
std::unique_ptr<snip::AnnotationOverlay> g_annotation;

bool Busy()
{
    return (g_selection && g_selection->IsActive()) || (g_annotation && g_annotation->IsActive());
}

void BeginAnnotationFromSelection()
{
    // Crop from the freeze taken before the overlay — not a second live BitBlt.
    auto cropped = snip::SelectionOverlay::LastCrop();
    if (cropped.Empty())
    {
        MessageBoxW(
            g_mainWnd,
            L"选区裁切失败（可能选区无效）。请重新截图。",
            L"SnipClone",
            MB_ICONERROR | MB_OK);
        return;
    }

    g_annotation = std::make_unique<snip::AnnotationOverlay>();
    if (!g_annotation->Start(g_instance, g_mainWnd, std::move(cropped)))
    {
        g_annotation.reset();
        MessageBoxW(g_mainWnd, L"Failed to open annotation overlay.", L"SnipClone", MB_ICONERROR | MB_OK);
    }
}

void BeginSelection()
{
    if (Busy())
    {
        return;
    }

    // PinShot: grab virtual desktop first, then show overlay on that freeze.
    auto freeze = snip::ScreenCapture::CaptureVirtualScreen();
    if (freeze.Empty())
    {
        MessageBoxW(g_mainWnd, L"Screen capture failed.", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }

    g_selection = std::make_unique<snip::SelectionOverlay>();
    if (!g_selection->Start(g_instance, g_mainWnd, std::move(freeze)))
    {
        g_selection.reset();
        MessageBoxW(g_mainWnd, L"Failed to create selection overlay.", L"SnipClone", MB_ICONERROR | MB_OK);
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
        if (g_selection && !g_selection->IsActive())
        {
            g_selection.reset();
        }
        if (wParam != 0)
        {
            // PinShot: capture then annotate. SnipClone: region capture then annotate host.
            BeginAnnotationFromSelection();
        }
        return 0;

    case snip::WM_SNIP_ANNOTATION_DONE:
        if (g_annotation && !g_annotation->IsActive())
        {
            g_annotation.reset();
        }
        return 0;

    case WM_DESTROY:
        if (g_selection)
        {
            if (g_selection->IsActive())
            {
                g_selection->Stop(false);
            }
            g_selection.reset();
        }
        if (g_annotation)
        {
            if (g_annotation->IsActive())
            {
                g_annotation->Stop();
            }
            g_annotation.reset();
        }
        snip::PinManager::Instance().CloseAll();
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

int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE, PWSTR, int)
{
    g_instance = hInstance;

    using SetDpiAwFn = BOOL(WINAPI*)(DPI_AWARENESS_CONTEXT);
    if (auto setDpi = reinterpret_cast<SetDpiAwFn>(
            GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext")))
    {
        setDpi(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

    if (!RegisterMainClass(hInstance))
    {
        return 1;
    }

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

    // Letter keys are 'A'..'Z' (0x41..0x5A); VK_S is not a defined macro in all SDKs.
    if (!snip::GlobalHotkey::Register(static_cast<UINT>('S'), MOD_CONTROL | MOD_SHIFT, kHotkeySnip, g_mainWnd))
    {
        MessageBoxW(
            nullptr,
            L"Failed to register hotkey Ctrl+Shift+S.\nAnother app may already own it.",
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
    snip::PinManager::Instance().CloseAll();
    CoUninitialize();
    return static_cast<int>(msg.wParam);
}
