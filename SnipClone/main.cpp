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
#include "AnnotationOverlay/AnnotatorDocument.h"
#include "Capture/ScreenCapture.h"
#include "Common/AppSettings.h"
#include "Common/ImageUtil.h"
#include "Common/ProjectIO.h"
#include "Common/SettingsDialog.h"
#include "Common/SingleInstance.h"
#include "Common/TrayIcon.h"
#include "Hotkey/GlobalHotkey.h"
#include "PinWindow/PinManager.h"
#include "PinWindow/PinWindow.h"
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
bool g_snipHotkeyOk = false;
bool g_pinsHotkeyOk = false;
UINT g_taskbarCreatedMsg = 0;

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

void BeginAnnotationFromDocument(snip::AnnotatorDocument doc)
{
    if (Busy())
    {
        MessageBoxW(g_mainWnd, L"请先结束当前截图或编辑。", L"SnipClone", MB_ICONINFORMATION | MB_OK);
        return;
    }
    if (!doc.Editable())
    {
        MessageBoxW(g_mainWnd, L"项目无效或已损坏。", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    g_annotation = std::make_unique<snip::AnnotationOverlay>();
    if (!g_annotation->StartFromDocument(g_instance, g_mainWnd, std::move(doc), nullptr))
    {
        g_annotation.reset();
        MessageBoxW(g_mainWnd, L"无法打开项目。", L"SnipClone", MB_ICONERROR | MB_OK);
    }
}

void OpenProjectFile()
{
    std::wstring path;
    if (!snip::ProjectIO::BrowseOpenPath(g_mainWnd, path))
    {
        return;
    }
    snip::AnnotatorDocument doc;
    std::wstring err;
    if (!snip::ProjectIO::Load(path, doc, &err))
    {
        MessageBoxW(g_mainWnd, err.empty() ? L"打开项目失败。" : err.c_str(), L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    snip::ProjectIO::RememberRecent(path);
    BeginAnnotationFromDocument(std::move(doc));
}

void OpenImageForAnnotate()
{
    // M3: plain image import — only new annotations; no recovered vectors from PNG.
    const std::wstring path = snip::ImageUtil::BrowseOpenImage(g_mainWnd, snip::LiveSettings().outputDir);
    if (path.empty())
    {
        return;
    }
    snip::CaptureResult image;
    if (!snip::ImageUtil::LoadImageFile(path, image) || image.Empty())
    {
        MessageBoxW(g_mainWnd, L"无法打开该图片。", L"SnipClone", MB_ICONERROR | MB_OK);
        return;
    }
    MessageBoxW(
        g_mainWnd,
        L"正在打开普通图片进行标注。\n\n"
        L"这与「打开项目 (.scdoc)」不同：图片里没有可恢复的原有箭头/文字对象，"
        L"只能新增批注。若需要跨次修改同一套对象，请使用「保存可编辑项目」。",
        L"SnipClone",
        MB_ICONINFORMATION | MB_OK);

    snip::AnnotatorDocument doc;
    doc.base = image;
    doc.freeze = std::move(image);
    doc.shapes.clear();
    BeginAnnotationFromDocument(std::move(doc));
}

void RestoreAutosaveProject()
{
    snip::AnnotatorDocument doc;
    std::wstring err;
    if (!snip::ProjectIO::LoadAutosave(doc, &err))
    {
        MessageBoxW(
            g_mainWnd,
            err.empty() ? L"没有可恢复的自动保存。" : err.c_str(),
            L"SnipClone",
            MB_ICONINFORMATION | MB_OK);
        return;
    }
    BeginAnnotationFromDocument(std::move(doc));
}

void BeginAnnotationFromPin(snip::PinWindow* pin)
{
    if (!pin || !snip::PinManager::Instance().Owns(pin) || !pin->HasEditableDocument())
    {
        if (pin)
        {
            pin->SetVisible(true);
        }
        return;
    }
    if (Busy())
    {
        pin->SetVisible(true);
        return;
    }
    snip::AnnotatorDocument doc = pin->Document();
    g_annotation = std::make_unique<snip::AnnotationOverlay>();
    if (!g_annotation->StartFromDocument(g_instance, g_mainWnd, std::move(doc), pin))
    {
        g_annotation.reset();
        pin->SetVisible(true);
        MessageBoxW(g_mainWnd, L"无法打开贴图编辑。", L"SnipClone", MB_ICONERROR | MB_OK);
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
    if (snip::LiveSettings().outputDir.empty())
    {
        wchar_t docs[MAX_PATH]{};
        if (SUCCEEDED(SHGetFolderPathW(nullptr, CSIDL_MYDOCUMENTS, nullptr, SHGFP_TYPE_CURRENT, docs)))
        {
            snip::LiveSettings().outputDir = std::wstring(docs) + L"\\SnipClone";
            CreateDirectoryW(snip::LiveSettings().outputDir.c_str(), nullptr);
            snip::LiveSettings().Save();
        }
    }
    OpenDirectory(snip::LiveSettings().outputDir);
}

void ShowAbout()
{
    std::wstring body = L"SnipClone ";
    body += kVersion;
    body += L"\n\n截图: ";
    body += snip::AppSettings::FormatHotkey(snip::LiveSettings().snip);
    if (!g_snipHotkeyOk)
    {
        body += L" （注册失败）";
    }
    body += L"\n贴图穿透退出: ";
    body += snip::AppSettings::FormatHotkey(snip::LiveSettings().pins);
    if (!g_pinsHotkeyOk)
    {
        body += L" （注册失败）";
    }
    body += L"\n\n设置文件:\n";
    body += snip::AppSettings::SettingsPath();
    body += L"\n\n最低系统: Windows 10 x64\n依赖: 系统自带 Direct2D / WIC / DWrite";
    MessageBoxW(g_mainWnd, body.c_str(), L"关于 SnipClone", MB_ICONINFORMATION | MB_OK);
}

bool g_smokeMode = false;

void OnTrayCommand(snip::TrayIcon::Command cmd);
bool RegisterConfiguredHotkeys(bool showConflictUi);

void RefreshTrayTip()
{
    if (!g_tray)
    {
        return;
    }
    std::wstring tip = L"SnipClone — ";
    tip += snip::AppSettings::FormatHotkey(snip::LiveSettings().snip);
    g_tray->Destroy();
    if (!g_tray->Create(g_mainWnd, g_instance, tip, OnTrayCommand))
    {
        g_tray.reset();
        return;
    }
    g_tray->SetClickThroughHotkeyTip(snip::AppSettings::FormatHotkey(snip::LiveSettings().pins));
}

void OpenSettings()
{
    if (snip::ShowSettingsDialog(g_mainWnd, g_instance))
    {
        snip::GlobalHotkey::Unregister(kHotkeySnip, g_mainWnd);
        snip::GlobalHotkey::Unregister(kHotkeyPins, g_mainWnd);
        RegisterConfiguredHotkeys(true);
        RefreshTrayTip();
    }
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
    case snip::TrayIcon::Command::RestoreLastPin:
        snip::PinManager::Instance().RestoreLastClosed(g_instance);
        break;
    case snip::TrayIcon::Command::Settings:
        OpenSettings();
        break;
    case snip::TrayIcon::Command::OpenProject:
        OpenProjectFile();
        break;
    case snip::TrayIcon::Command::OpenImage:
        OpenImageForAnnotate();
        break;
    case snip::TrayIcon::Command::RestoreAutosave:
        RestoreAutosaveProject();
        break;
    case snip::TrayIcon::Command::ClearAutosave:
        if (snip::ProjectIO::ClearAutosave())
        {
            MessageBoxW(g_mainWnd, L"已清除自动保存。", L"SnipClone", MB_ICONINFORMATION | MB_OK);
        }
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

bool RegisterConfiguredHotkeys(bool showConflictUi)
{
    g_snipHotkeyOk = snip::GlobalHotkey::Register(
        snip::LiveSettings().snip.vk, snip::LiveSettings().snip.mods, kHotkeySnip, g_mainWnd);
    g_pinsHotkeyOk = snip::GlobalHotkey::Register(
        snip::LiveSettings().pins.vk, snip::LiveSettings().pins.mods, kHotkeyPins, g_mainWnd);

    // Smoke / headless: never block on modal conflict dialogs (plan parallel fix).
    if (showConflictUi && !g_smokeMode)
    {
        if (!g_snipHotkeyOk)
        {
            std::wstring msg = L"热键 ";
            msg += snip::AppSettings::FormatHotkey(snip::LiveSettings().snip);
            msg += L" 暂不可用，请换一组按键。\n可在托盘「设置…」中重新录制。";
            MessageBoxW(g_mainWnd, msg.c_str(), L"SnipClone", MB_ICONWARNING | MB_OK);
        }
        if (!g_pinsHotkeyOk)
        {
            std::wstring msg = L"热键 ";
            msg += snip::AppSettings::FormatHotkey(snip::LiveSettings().pins);
            msg += L" 暂不可用（贴图穿透退出）。\n可在托盘「设置…」中重新录制。";
            MessageBoxW(g_mainWnd, msg.c_str(), L"SnipClone", MB_ICONWARNING | MB_OK);
        }
    }
    return g_snipHotkeyOk || g_pinsHotkeyOk;
}

LRESULT CALLBACK MainWndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    if (g_tray && g_tray->HandleMessage(msg, wParam, lParam))
    {
        return 0;
    }

    if (g_taskbarCreatedMsg && msg == g_taskbarCreatedMsg)
    {
        // A13: Explorer restarted — recreate tray icon.
        if (g_tray)
        {
            g_tray->Recreate();
        }
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

    case snip::WM_SNIP_PIN_EDIT:
        BeginAnnotationFromPin(reinterpret_cast<snip::PinWindow*>(lParam));
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
    g_smokeMode = smoke;

    snip::SingleInstance single;
    if (single.AlreadyRunning())
    {
        if (smoke)
        {
            // A14: smoke must not report success when another instance owns the lock.
            return 2;
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

    snip::LoadLiveSettings(); // A04: single in-process settings instance
    // Ensure settings file exists with defaults on first run.
    snip::LiveSettings().Save();

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

    RegisterConfiguredHotkeys(!smoke);

    g_taskbarCreatedMsg = RegisterWindowMessageW(L"TaskbarCreated");

    g_tray = std::make_unique<snip::TrayIcon>();
    std::wstring tip = L"SnipClone — ";
    tip += snip::AppSettings::FormatHotkey(snip::LiveSettings().snip);
    if (!g_tray->Create(g_mainWnd, hInstance, tip, OnTrayCommand))
    {
        // Headless CI / no shell: continue without tray.
        g_tray.reset();
    }
    else
    {
        g_tray->SetClickThroughHotkeyTip(snip::AppSettings::FormatHotkey(snip::LiveSettings().pins));
    }

    if (smoke)
    {
        // A14 smoke contract:
        //   required → main window created, settings loaded/saved
        //   optional → tray (may fail headless), hotkeys (conflict OK unless --smoke-strict)
        //   exit 0 success | 1 hard fail | 2 already running | 3 strict hotkey miss
        const bool strict = HasArg(cmdLine, L"--smoke-strict");
        const bool settingsOk = !snip::AppSettings::SettingsPath().empty();
        int code = (g_mainWnd && settingsOk) ? 0 : 1;
        if (code == 0 && strict && !g_snipHotkeyOk)
        {
            code = 3;
        }
        if (g_tray)
        {
            g_tray->Destroy();
            g_tray.reset();
        }
        snip::GlobalHotkey::Unregister(kHotkeyPins, g_mainWnd);
        snip::GlobalHotkey::Unregister(kHotkeySnip, g_mainWnd);
        DestroyWindow(g_mainWnd);
        CoUninitialize();
        return code;
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
    snip::LiveSettings().Save();
    CoUninitialize();
    return static_cast<int>(msg.wParam);
}
