//==============================================================================
// SnipClone — single-instance guard (named mutex)
//==============================================================================
#pragma once

#include "WinCompat.h"

namespace snip
{

class SingleInstance
{
public:
    explicit SingleInstance(const wchar_t* mutexName = L"Local\\SnipClone.SingleInstance.v1")
    {
        m_mutex = CreateMutexW(nullptr, TRUE, mutexName);
        m_alreadyRunning = (m_mutex != nullptr && GetLastError() == ERROR_ALREADY_EXISTS);
    }

    ~SingleInstance()
    {
        if (m_mutex)
        {
            ReleaseMutex(m_mutex);
            CloseHandle(m_mutex);
            m_mutex = nullptr;
        }
    }

    SingleInstance(const SingleInstance&) = delete;
    SingleInstance& operator=(const SingleInstance&) = delete;

    bool AlreadyRunning() const { return m_alreadyRunning; }

    // Ask an existing main window to surface (tray tip / foreground).
    static bool ActivateExisting(const wchar_t* mainClassName)
    {
        HWND hwnd = FindWindowW(mainClassName, nullptr);
        if (!hwnd)
        {
            return false;
        }
        PostMessageW(hwnd, WM_APP + 0x70, 0, 0); // WM_SNIP_ACTIVATE
        return true;
    }

private:
    HANDLE m_mutex = nullptr;
    bool m_alreadyRunning = false;
};

inline constexpr UINT WM_SNIP_ACTIVATE = WM_APP + 0x70;

} // namespace snip
