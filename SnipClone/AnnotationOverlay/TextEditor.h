//==============================================================================
// SnipClone — floating text editor（对齐 参考工具 / PinShot：落点浮动输入框）
//
// 使用 Win32 EDIT，天然支持中文 IME。Enter 提交，Esc 取消。
//==============================================================================
#pragma once

#include "DrawCommand.h"

#include "../Common/WinCompat.h"

#include <functional>
#include <string>

namespace snip
{

class TextEditor
{
public:
    using DoneFn = std::function<void(bool committed, const std::wstring& text)>;

    TextEditor() = default;
    ~TextEditor();

    bool IsOpen() const { return m_host != nullptr; }

    // screenPos = top-left of text in screen coordinates.
    bool Open(
        HINSTANCE instance,
        HWND owner,
        POINT screenPos,
        const DrawStyle& style,
        const std::wstring& initial,
        DoneFn onDone);

    void ApplyStyle(const DrawStyle& style);
    void Close(bool commit);
    void Destroy();

    HWND Host() const { return m_host; }
    HWND Edit() const { return m_edit; }
    const DrawStyle& Style() const { return m_style; }

private:
    static LRESULT CALLBACK HostProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
    static LRESULT CALLBACK EditSubclass(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam, UINT_PTR id, DWORD_PTR data);

    void ResizeToContent();
    HFONT MakeFont(const DrawStyle& style) const;
    bool IsImeComposing() const;
    void ScheduleBlurCommit();

    static constexpr UINT WM_TE_BLUR_COMMIT = WM_APP + 0x71;

    HINSTANCE m_instance = nullptr;
    HWND m_owner = nullptr;
    HWND m_host = nullptr;
    HWND m_edit = nullptr;
    HFONT m_font = nullptr;
    DrawStyle m_style{};
    DoneFn m_onDone;
    bool m_closing = false;
};

} // namespace snip
