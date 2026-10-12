//==============================================================================
// SnipClone — entry / state machine
//
// 装配对齐 PinShot App.snip：
//   Idle → Capture(virtual freeze) → Selection(on freeze) → crop → Annotation
//        → copy / save / pin → Idle
//
// P4: tray + single-instance + settings.ini + --smoke
//==============================================================================
#include "AnnotationOverlay/AnnotationOverlay.h"
#include "Capture/ScreenCapture.h"
#include "Common/AppSettings.h"
#include "Common/SingleInstance.h"
#include "Common/TrayIcon.h"
#include "Hotkey/GlobalHotkey.h"
#include "PinWindow/PinManager.h"
#include "SelectionOverlay/SelectionOverlay.h"

#include "Common/WinCompat.h"

#include <shlobj.h>
#include <shellapi.h>

#include <memory>
#include <string>

#pragma comment(lib, "shell32.lib")

namespace
{
constexpr wchar_t kMainClass[] = L"SnipCloneMain";
constexpr int kHotkeySnip = 0x1001;
constexpr int kHotkeyPins = 0x1002;
constexpr wchar_t kVersion[] = L"0.2.0";

HINSTANCE g_instance = nullptr;
HWND g_mainWnd = nullptr;
std::unique_ptr<snip::SelectionOverlay> g_selection;
std::unique_ptr<snip::AnnotationOverlay> g_annotation;
std::unique_ptr<snip::TrayIcon> g_tray;
snip::AppSettings g_settings;
bool g_snipHotkeyOk = false;
bool g_pinsHotkeyOk = false;

bool Busy()
{
    return (g_selection && g_selection->IsActive()) || (g_annotation && g_annotation->IsActive());
}

void BeginAnnotationFromSelection()
{
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

    auto freeze = snip::SelectionOverlay::LastFreeze();
    g_annotation = std::make_unique<snip::AnnotationOverlay>();
    if (!g_annotation->Start(g_instance, g_mainWnd, std::move(cropped), std::move(freeze)))
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

void OpenDirectory(const std::wstring& path)
{
    if (path.empty())
    {
        return;
    }
    CreateDirectoryW(path.c_str(), nullptr);
    ShellExecuteW(nullptr, L"open", path.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
}

void ChooseOutputDir()
{
    // Minimal: open settings dir and tell user to edit OutputDir= in settings.ini
    // plus create default Documents\SnipClone if empty.
    if (g_settings.outputDir.empty())
    {
        wchar_t docs[MAX_PATH]{};
        if (SUCCEEDED(SHGetFolderPathW(nullptr, CSIDL_MYDOCUMENTS, nullptr, SHGFP_TYPE_CURRENT, docs)))
        {
            g_settings.outputDir = std::wstring(docs) + L"\\SnipClone";
            CreateDirectoryW(g_settings.outputDir.c_str(), nullptr);
            g_settings.Save();
        }
    }
    OpenDirectory(g_settings.outputDir);
}

void ShowAbout()
{
    std::wstring body = L"SnipClone ";
    body += kVersion;
    body += L"\n\n截图: ";
    body += snip::AppSettings::FormatHotkey(g_settings.snip);
    if (!g_snipHotkeyOk)
    {
        body += L" （注册失败）";
    }
    body += L"\n贴图穿透退出: ";
    body += snip::AppSettings::FormatHotkey(g_settings.pins);
    if (!g_pinsHotkeyOk)
    {
        body += L" （注册失败）";
    }
    body += L"\n\n设置文件:\n";
    body += snip::AppSettings::SettingsPath();
    body += L"\n\n最低系统: Windows 10 x64\n依赖: 系统自带 Direct2D / WIC / DWrite";
    MessageBoxW(g_mainWnd, body.c_str(), L"关于 SnipClone", MB_ICONINFORMATION | MB_OK);
}

void OnTrayCommand(snip::TrayIcon::Command cmd)
{
    switch (cmd)
    {
    case snip::TrayIcon::Command::Snip:
        BeginSelection();
        break;
    case snip::TrayIcon::Command::ShowPins:
        snip::PinManager::Instance().ShowAll();
        break;
    case snip::TrayIcon::Command::HidePins:
        snip::PinManager::Instance().HideAll();
        break;
    case snip::TrayIcon::Command::ClearClickThrough:
        snip::PinManager::Instance().ClearClickThroughAll();
        break;
    case snip::TrayIcon::Command::OpenOutputDir:
        ChooseOutputDir();
        break;
    case snip::TrayIcon::Command::OpenSettingsDir:
        CreateDirectoryW(snip::AppSettings::SettingsDir().c_str(), nullptr);
        OpenDirectory(snip::AppSettings::SettingsDir());
        break;
    case snip::TrayIcon::Command::About:
        ShowAbout();
        break;
    case snip::TrayIcon::Command::Exit:
        DestroyWindow(g_mainWnd);
        break;
    }
}

bool RegisterConfiguredHotkeys()
{
    g_snipHotkeyOk = snip::GlobalHotkey::Register(
        g_settings.snip.vk, g_settings.snip.mods, kHotkeySnip, g_mainWnd);
    g_pinsHotkeyOk = snip::GlobalHotkey::Register(
        g_settings.pins.vk, g_settings.pins.mods, kHotkeyPins, g_mainWnd);

    if (!g_snipHotkeyOk)
    {
        std::wstring msg = L"热键 ";
        msg += snip::AppSettings::FormatHotkey(g_settings.snip);
        msg += L" 注册失败，可能被其他程序占用。\n可编辑 %APPDATA%\\SnipClone\\settings.ini 后重启。";
        MessageBoxW(nullptr, msg.c_str(), L"SnipClone", MB_ICONWARNING | MB_OK);
    }
    if (!g_pinsHotkeyOk)
    {
        std::wstring msg = L"热键 ";
        msg += snip::AppSettings::FormatHotkey(g_settings.pins);
        msg += L" 注册失败（贴图穿透退出）。";
        MessageBoxW(nullptr, msg.c_str(), L"SnipClone", MB_ICONWARNING | MB_OK);
    }
    return g_snipHotkeyOk || g_pinsHotkeyOk;
}

LRESULT CALLBACK MainWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (g_tray && g_tray->HandleMessage(msg, wParam, lParam))
    {
        return 0;
    }

    switch (msg)
    {
    case snip::WM_SNIP_ACTIVATE:
        if (g_tray)
        {
            g_tray->ShowBalloon(L"SnipClone", L"已在运行中（托盘图标）。");
        }
        return 0;

    case WM_HOTKEY:
        if (static_cast<int>(wParam) == kHotkeySnip)
        {
            BeginSelection();
        }
        else if (static_cast<int>(wParam) == kHotkeyPins)
        {
            snip::PinManager::Instance().ClearClickThroughAll();
        }
        return 0;

    case snip::WM_SNIP_SELECTION_DONE:
        if (g_selection && !g_selection->IsActive())
        {
            g_selection.reset();
        }
        if (wParam != 0)
        {
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
        if (g_tray)
        {
            g_tray->Destroy();
            g_tray.reset();
        }
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

bool HasArg(LPWSTR cmdLine, const wchar_t* flag)
{
    if (!cmdLine || !flag)
    {
        return false;
    }
    std::wstring line(cmdLine);
    return line.find(flag) != std::wstring::npos;
}
} // namespace

int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE, PWSTR cmdLine, int)
{
    g_instance = hInstance;
    const bool smoke = HasArg(cmdLine, L"--smoke");

    snip::SingleInstance single;
    if (single.AlreadyRunning())
    {
        if (smoke)
        {
            // Second smoke instance in CI should still exit 0 after detecting lock.
            return 0;
        }
        if (!snip::SingleInstance::ActivateExisting(kMainClass))
        {
            MessageBoxW(
                nullptr,
                L"SnipClone 已在运行。\n请使用托盘图标或热键。",
                L"SnipClone",
                MB_ICONINFORMATION | MB_OK);
        }
        return 0;
    }

    using SetDpiAwFn = BOOL(WINAPI*)(DPI_AWARENESS_CONTEXT);
    if (auto setDpi = reinterpret_cast<SetDpiAwFn>(
            GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext")))
    {
        setDpi(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

    g_settings = snip::AppSettings::Load();
    // Ensure settings file exists with defaults on first run.
    g_settings.Save();

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

    RegisterConfiguredHotkeys();

    g_tray = std::make_unique<snip::TrayIcon>();
    std::wstring tip = L"SnipClone — ";
    tip += snip::AppSettings::FormatHotkey(g_settings.snip);
    if (!g_tray->Create(g_mainWnd, hInstance, tip, OnTrayCommand))
    {
        // Headless CI / no shell: continue without tray.
        g_tray.reset();
    }

    if (smoke)
    {
        // Startup smoke: init path succeeded; tear down cleanly for CI.
        if (g_tray)
        {
            g_tray->Destroy();
            g_tray.reset();
        }
        snip::GlobalHotkey::Unregister(kHotkeyPins, g_mainWnd);
        snip::GlobalHotkey::Unregister(kHotkeySnip, g_mainWnd);
        DestroyWindow(g_mainWnd);
        CoUninitialize();
        return 0;
    }

    MSG msg{};
    while (GetMessageW(&msg, nullptr, 0, 0) > 0)
    {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    snip::GlobalHotkey::Unregister(kHotkeyPins, g_mainWnd);
    snip::GlobalHotkey::Unregister(kHotkeySnip, g_mainWnd);
    snip::PinManager::Instance().CloseAll();
    g_settings.Save();
    CoUninitialize();
    return static_cast<int>(msg.wParam);
}
