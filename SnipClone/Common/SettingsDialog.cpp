//==============================================================================
// SnipClone — settings dialog (resource-less Win32)
//==============================================================================
#include "SettingsDialog.h"

#include "AppSettings.h"
#include "../AnnotationOverlay/DrawCommand.h"

#include <commdlg.h>
#include <shlobj.h>

#include <string>

#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "ole32.lib")

namespace snip
{
namespace
{
constexpr wchar_t kClass[] = L"SnipCloneSettingsDlg";
constexpr wchar_t kRunKey[] = L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";
constexpr wchar_t kRunValue[] = L"SnipClone";

enum Id : int
{
    IdSnipLabel = 1001,
    IdSnipValue,
    IdSnipRecord,
    IdPinsLabel,
    IdPinsValue,
    IdPinsRecord,
    IdRestoreHotkeys,
    IdOutLabel,
    IdOutEdit,
    IdOutBrowse,
    IdAutostart,
    IdWelcomeTip,
    IdSelectAfterDraw,
    IdCloseAfterExport,
    IdDefaultToolLabel,
    IdDefaultTool,
    IdApply,
    IdOk,
    IdCancel,
    IdStatus,
};

struct DefaultToolOpt
{
    ToolKind kind;
    const wchar_t* label;
};

constexpr DefaultToolOpt kDefaultTools[] = {
    { ToolKind::Select, L"选择" },
    { ToolKind::Rect, L"矩形" },
    { ToolKind::Ellipse, L"椭圆" },
    { ToolKind::Arrow, L"箭头" },
    { ToolKind::Pen, L"画笔" },
    { ToolKind::Mosaic, L"马赛克" },
    { ToolKind::Text, L"文字" },
    { ToolKind::Eraser, L"橡皮" },
};

struct DlgState
{
    HINSTANCE instance = nullptr;
    AppSettings draft{};
    int recording = 0; // 0 none, 1 snip, 2 pins
    HWND hwnd = nullptr;
    bool applied = false;
    bool done = false;
};

void SetStaticText(HWND hwnd, int id, const std::wstring& text)
{
    SetWindowTextW(GetDlgItem(hwnd, id), text.c_str());
}

void RefreshHotkeyLabels(HWND hwnd, const AppSettings& s)
{
    SetStaticText(hwnd, IdSnipValue, AppSettings::FormatHotkey(s.snip));
    SetStaticText(hwnd, IdPinsValue, AppSettings::FormatHotkey(s.pins));
}

void SetStatus(HWND hwnd, const wchar_t* text)
{
    SetStaticText(hwnd, IdStatus, text ? text : L"");
}

void FillDefaultToolCombo(HWND combo, UINT selected)
{
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    int selIndex = 0;
    for (int i = 0; i < static_cast<int>(sizeof(kDefaultTools) / sizeof(kDefaultTools[0])); ++i)
    {
        const auto& opt = kDefaultTools[i];
        SendMessageW(combo, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(opt.label));
        SendMessageW(combo, CB_SETITEMDATA, i, static_cast<LPARAM>(opt.kind));
        if (static_cast<UINT>(opt.kind) == selected)
        {
            selIndex = i;
        }
    }
    SendMessageW(combo, CB_SETCURSEL, selIndex, 0);
}

UINT ReadDefaultToolCombo(HWND combo)
{
    const int idx = static_cast<int>(SendMessageW(combo, CB_GETCURSEL, 0, 0));
    if (idx < 0)
    {
        return static_cast<UINT>(ToolKind::Select);
    }
    const LPARAM data = SendMessageW(combo, CB_GETITEMDATA, idx, 0);
    return static_cast<UINT>(data);
}

bool BrowseFolder(HWND owner, std::wstring& pathOut)
{
    BROWSEINFOW bi{};
    bi.hwndOwner = owner;
    bi.lpszTitle = L"选择输出目录";
    bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
    PIDLIST_ABSOLUTE pidl = SHBrowseForFolderW(&bi);
    if (!pidl)
    {
        return false;
    }
    wchar_t buf[MAX_PATH]{};
    const BOOL ok = SHGetPathFromIDListW(pidl, buf);
    CoTaskMemFree(pidl);
    if (!ok || !buf[0])
    {
        return false;
    }
    pathOut = buf;
    return true;
}

bool ApplyDraft(HWND hwnd, DlgState* st)
{
    wchar_t outBuf[MAX_PATH]{};
    GetWindowTextW(GetDlgItem(hwnd, IdOutEdit), outBuf, MAX_PATH);
    st->draft.outputDir = outBuf;
    st->draft.startWithWindows =
        (SendMessageW(GetDlgItem(hwnd, IdAutostart), BM_GETCHECK, 0, 0) == BST_CHECKED);
    st->draft.showWelcomeTip =
        (SendMessageW(GetDlgItem(hwnd, IdWelcomeTip), BM_GETCHECK, 0, 0) == BST_CHECKED);
    st->draft.selectAfterDraw =
        (SendMessageW(GetDlgItem(hwnd, IdSelectAfterDraw), BM_GETCHECK, 0, 0) == BST_CHECKED);
    st->draft.closeAfterExport =
        (SendMessageW(GetDlgItem(hwnd, IdCloseAfterExport), BM_GETCHECK, 0, 0) == BST_CHECKED);
    st->draft.defaultTool = ReadDefaultToolCombo(GetDlgItem(hwnd, IdDefaultTool));

    const AppSettings previous = LiveSettings();
    LiveSettings() = st->draft;
    if (!LiveSettings().Save())
    {
        LiveSettings() = previous;
        SetStatus(hwnd, L"保存设置失败，已保留原配置。");
        return false;
    }

    std::wstring autoErr;
    if (!ApplyStartWithWindows(st->draft.startWithWindows, &autoErr))
    {
        SetStatus(hwnd, autoErr.empty() ? L"自启动注册失败。" : autoErr.c_str());
    }
    else
    {
        SetStatus(hwnd, L"已应用。");
    }
    st->applied = true;
    return true;
}

LRESULT CALLBACK DlgProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    auto* st = reinterpret_cast<DlgState*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    switch (msg)
    {
    case WM_CREATE:
    {
        auto* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
        st = static_cast<DlgState*>(cs->lpCreateParams);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(st));
        st->hwnd = hwnd;

        CreateWindowExW(0, L"STATIC", L"截图热键", WS_CHILD | WS_VISIBLE, 16, 16, 80, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdSnipLabel)), st->instance, nullptr);
        CreateWindowExW(0, L"STATIC", L"", WS_CHILD | WS_VISIBLE | SS_LEFT, 100, 16, 160, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdSnipValue)), st->instance, nullptr);
        CreateWindowExW(0, L"BUTTON", L"录制…", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 270, 12, 80, 28, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdSnipRecord)), st->instance, nullptr);

        CreateWindowExW(0, L"STATIC", L"穿透退出", WS_CHILD | WS_VISIBLE, 16, 52, 80, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdPinsLabel)), st->instance, nullptr);
        CreateWindowExW(0, L"STATIC", L"", WS_CHILD | WS_VISIBLE | SS_LEFT, 100, 52, 160, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdPinsValue)), st->instance, nullptr);
        CreateWindowExW(0, L"BUTTON", L"录制…", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 270, 48, 80, 28, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdPinsRecord)), st->instance, nullptr);

        CreateWindowExW(0, L"BUTTON", L"恢复默认热键", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 360, 30, 110, 28,
                        hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdRestoreHotkeys)),
                        st->instance, nullptr);

        CreateWindowExW(0, L"STATIC", L"输出目录", WS_CHILD | WS_VISIBLE, 16, 96, 80, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdOutLabel)), st->instance, nullptr);
        CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", st->draft.outputDir.c_str(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL, 100, 92, 250, 26, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdOutEdit)), st->instance, nullptr);
        CreateWindowExW(0, L"BUTTON", L"浏览…", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 360, 90, 110, 28, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdOutBrowse)), st->instance, nullptr);

        CreateWindowExW(0, L"BUTTON", L"开机启动", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
                        16, 136, 120, 24, hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdAutostart)),
                        st->instance, nullptr);
        SendMessageW(GetDlgItem(hwnd, IdAutostart), BM_SETCHECK,
                     st->draft.startWithWindows ? BST_CHECKED : BST_UNCHECKED, 0);

        CreateWindowExW(0, L"BUTTON", L"启动时显示操作提示",
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX, 150, 136, 200, 24, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdWelcomeTip)), st->instance, nullptr);
        SendMessageW(GetDlgItem(hwnd, IdWelcomeTip), BM_SETCHECK,
                     st->draft.showWelcomeTip ? BST_CHECKED : BST_UNCHECKED, 0);

        CreateWindowExW(0, L"STATIC", L"默认工具", WS_CHILD | WS_VISIBLE, 16, 172, 80, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdDefaultToolLabel)), st->instance,
                        nullptr);
        CreateWindowExW(0, L"COMBOBOX", L"",
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL, 100, 168, 160, 200,
                        hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdDefaultTool)), st->instance,
                        nullptr);
        FillDefaultToolCombo(GetDlgItem(hwnd, IdDefaultTool), st->draft.defaultTool);

        CreateWindowExW(0, L"BUTTON", L"标注完成后切换到选择工具",
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX, 16, 208, 280, 24, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdSelectAfterDraw)), st->instance,
                        nullptr);
        SendMessageW(GetDlgItem(hwnd, IdSelectAfterDraw), BM_SETCHECK,
                     st->draft.selectAfterDraw ? BST_CHECKED : BST_UNCHECKED, 0);

        CreateWindowExW(0, L"BUTTON", L"复制/保存成功后关闭标注",
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX, 16, 236, 280, 24, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdCloseAfterExport)), st->instance,
                        nullptr);
        SendMessageW(GetDlgItem(hwnd, IdCloseAfterExport), BM_SETCHECK,
                     st->draft.closeAfterExport ? BST_CHECKED : BST_UNCHECKED, 0);

        CreateWindowExW(0, L"STATIC", L"", WS_CHILD | WS_VISIBLE, 16, 272, 450, 22, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdStatus)), st->instance, nullptr);

        CreateWindowExW(0, L"BUTTON", L"应用", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 200, 308, 80, 30, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdApply)), st->instance, nullptr);
        CreateWindowExW(0, L"BUTTON", L"确定", WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON, 290,
                        308, 80, 30, hwnd, reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdOk)), st->instance,
                        nullptr);
        CreateWindowExW(0, L"BUTTON", L"取消", WS_CHILD | WS_VISIBLE | WS_TABSTOP, 380, 308, 80, 30, hwnd,
                        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IdCancel)), st->instance, nullptr);

        RefreshHotkeyLabels(hwnd, st->draft);
        SetStatus(hwnd, L"点击「录制」后按下新的快捷键组合。");
        return 0;
    }
    case WM_COMMAND:
    {
        if (!st)
        {
            break;
        }
        const int id = LOWORD(wParam);
        if (id == IdSnipRecord)
        {
            st->recording = 1;
            SetStatus(hwnd, L"请按下新的截图热键…");
            SetFocus(hwnd);
            return 0;
        }
        if (id == IdPinsRecord)
        {
            st->recording = 2;
            SetStatus(hwnd, L"请按下新的穿透退出热键…");
            SetFocus(hwnd);
            return 0;
        }
        if (id == IdRestoreHotkeys)
        {
            st->draft.RestoreDefaultHotkeys();
            st->recording = 0;
            RefreshHotkeyLabels(hwnd, st->draft);
            SetStatus(hwnd, L"已恢复默认热键，点击「应用」或「确定」生效。");
            return 0;
        }
        if (id == IdOutBrowse)
        {
            std::wstring path = st->draft.outputDir;
            if (BrowseFolder(hwnd, path))
            {
                st->draft.outputDir = path;
                SetWindowTextW(GetDlgItem(hwnd, IdOutEdit), path.c_str());
            }
            return 0;
        }
        if (id == IdApply)
        {
            ApplyDraft(hwnd, st);
            return 0;
        }
        if (id == IdOk)
        {
            if (ApplyDraft(hwnd, st))
            {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        if (id == IdCancel || id == IDCANCEL)
        {
            DestroyWindow(hwnd);
            return 0;
        }
        break;
    }
    case WM_KEYDOWN:
    case WM_SYSKEYDOWN:
    {
        if (!st || st->recording == 0)
        {
            break;
        }
        const UINT vk = static_cast<UINT>(wParam);
        if (vk == VK_CONTROL || vk == VK_SHIFT || vk == VK_MENU || vk == VK_LWIN || vk == VK_RWIN ||
            vk == VK_ESCAPE)
        {
            if (vk == VK_ESCAPE)
            {
                st->recording = 0;
                SetStatus(hwnd, L"已取消录制。");
            }
            return 0;
        }
        HotkeySpec hk{};
        hk.vk = vk;
        if (GetKeyState(VK_CONTROL) & 0x8000)
            hk.mods |= MOD_CONTROL;
        if (GetKeyState(VK_SHIFT) & 0x8000)
            hk.mods |= MOD_SHIFT;
        if (GetKeyState(VK_MENU) & 0x8000)
            hk.mods |= MOD_ALT;
        if ((GetKeyState(VK_LWIN) & 0x8000) || (GetKeyState(VK_RWIN) & 0x8000))
            hk.mods |= MOD_WIN;
        if (hk.mods == 0)
        {
            SetStatus(hwnd, L"请配合 Ctrl / Shift / Alt 使用。");
            return 0;
        }
        if (st->recording == 1)
        {
            st->draft.snip = hk;
        }
        else
        {
            st->draft.pins = hk;
        }
        st->recording = 0;
        RefreshHotkeyLabels(hwnd, st->draft);
        SetStatus(hwnd, L"已录制，点击「应用」或「确定」生效。");
        return 0;
    }
    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;
    case WM_DESTROY:
        if (st)
        {
            st->hwnd = nullptr;
            st->done = true;
        }
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}
} // namespace

bool IsStartWithWindowsRegistered()
{
    HKEY key = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kRunKey, 0, KEY_READ, &key) != ERROR_SUCCESS)
    {
        return false;
    }
    wchar_t buf[MAX_PATH]{};
    DWORD type = 0;
    DWORD cb = sizeof(buf);
    const LONG rc = RegQueryValueExW(key, kRunValue, nullptr, &type, reinterpret_cast<LPBYTE>(buf), &cb);
    RegCloseKey(key);
    return rc == ERROR_SUCCESS && type == REG_SZ && buf[0] != 0;
}

bool ApplyStartWithWindows(bool enable, std::wstring* outError)
{
    HKEY key = nullptr;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, kRunKey, 0, nullptr, 0, KEY_SET_VALUE | KEY_QUERY_VALUE, nullptr,
                        &key, nullptr) != ERROR_SUCCESS)
    {
        if (outError)
        {
            *outError = L"无法打开开机启动注册表项。";
        }
        return false;
    }
    LONG rc = ERROR_SUCCESS;
    if (enable)
    {
        wchar_t exe[MAX_PATH]{};
        GetModuleFileNameW(nullptr, exe, MAX_PATH);
        std::wstring value = L"\"";
        value += exe;
        value += L"\"";
        rc = RegSetValueExW(key, kRunValue, 0, REG_SZ, reinterpret_cast<const BYTE*>(value.c_str()),
                            static_cast<DWORD>((value.size() + 1) * sizeof(wchar_t)));
    }
    else
    {
        rc = RegDeleteValueW(key, kRunValue);
        if (rc == ERROR_FILE_NOT_FOUND)
        {
            rc = ERROR_SUCCESS;
        }
    }
    RegCloseKey(key);
    if (rc != ERROR_SUCCESS)
    {
        if (outError)
        {
            *outError = enable ? L"写入开机启动失败。" : L"清除开机启动失败。";
        }
        return false;
    }
    return true;
}

bool ShowSettingsDialog(HWND owner, HINSTANCE instance)
{
    static bool registered = false;
    if (!registered)
    {
        WNDCLASSW wc{};
        wc.lpfnWndProc = DlgProc;
        wc.hInstance = instance;
        wc.lpszClassName = kClass;
        wc.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
        wc.hCursor = LoadCursorW(nullptr, IDC_ARROW);
        RegisterClassW(&wc);
        registered = true;
    }

    DlgState st;
    st.instance = instance;
    st.draft = LiveSettings();
    st.draft.startWithWindows = IsStartWithWindowsRegistered();

    HWND hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
        kClass,
        L"SnipClone 设置",
        WS_CAPTION | WS_SYSMENU | WS_POPUP,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        500,
        390,
        owner,
        nullptr,
        instance,
        &st);
    if (!hwnd)
    {
        return false;
    }

    RECT rc{};
    GetWindowRect(hwnd, &rc);
    const int w = rc.right - rc.left;
    const int h = rc.bottom - rc.top;
    RECT base{};
    if (owner)
    {
        GetWindowRect(owner, &base);
    }
    else
    {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &base, 0);
    }
    const int x = base.left + ((base.right - base.left) - w) / 2;
    const int y = base.top + ((base.bottom - base.top) - h) / 2;
    SetWindowPos(hwnd, HWND_TOP, x, y, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW);

    if (owner)
    {
        EnableWindow(owner, FALSE);
    }

    MSG msg{};
    while (!st.done && GetMessageW(&msg, nullptr, 0, 0) > 0)
    {
        if (st.recording && (msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN) &&
            msg.hwnd != hwnd)
        {
            SendMessageW(hwnd, msg.message, msg.wParam, msg.lParam);
            continue;
        }
        if (!IsDialogMessageW(hwnd, &msg))
        {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if (owner)
    {
        EnableWindow(owner, TRUE);
        SetForegroundWindow(owner);
    }
    return st.applied;
}

} // namespace snip
