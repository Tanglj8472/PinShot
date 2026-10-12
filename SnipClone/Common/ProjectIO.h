//==============================================================================
// SnipClone — editable project file (.scdoc) + autosave (M3)
//==============================================================================
#pragma once

#include "../AnnotationOverlay/AnnotatorDocument.h"

#include "WinCompat.h"

#include <cstdint>
#include <string>
#include <vector>

namespace snip
{

struct ProjectIO
{
    static constexpr uint32_t kMagic = 0x31444353u; // 'SCD1' LE
    static constexpr uint32_t kVersion = 1;
    static constexpr size_t kMaxRecent = 8;
    static constexpr size_t kMaxImageBytes = 64u * 1024u * 1024u; // 64 MiB per buffer

    static std::wstring ProjectsDir();
    static std::wstring AutosavePath();
    static std::wstring RecentListPath();

    static bool Save(const AnnotatorDocument& doc, const std::wstring& path, std::wstring* err = nullptr);
    static bool Load(const std::wstring& path, AnnotatorDocument& out, std::wstring* err = nullptr);

    static bool SaveAutosave(const AnnotatorDocument& doc, std::wstring* err = nullptr);
    static bool LoadAutosave(AnnotatorDocument& out, std::wstring* err = nullptr);
    static bool HasAutosave();
    static bool ClearAutosave();

    static void RememberRecent(const std::wstring& path);
    static std::vector<std::wstring> ListRecent();
    static bool ClearRecent();

    // File dialogs (owner may be null).
    static bool BrowseSavePath(HWND owner, std::wstring& pathOut);
    static bool BrowseOpenPath(HWND owner, std::wstring& pathOut);
};

} // namespace snip
