//==============================================================================
// SnipClone — .scdoc project serialization
//==============================================================================
#include "ProjectIO.h"

#include "AppSettings.h"

#include <commdlg.h>
#include <shlobj.h>

#include <algorithm>
#include <cstdio>
#include <fstream>
#include <string>
#include <vector>

#pragma comment(lib, "comdlg32.lib")

namespace snip
{
namespace
{
std::wstring JoinPath(const std::wstring& a, const std::wstring& b)
{
    if (a.empty())
    {
        return b;
    }
    if (a.back() == L'\\' || a.back() == L'/')
    {
        return a + b;
    }
    return a + L'\\' + b;
}

bool WriteBytes(std::ofstream& out, const void* data, size_t n)
{
    out.write(reinterpret_cast<const char*>(data), static_cast<std::streamsize>(n));
    return static_cast<bool>(out);
}

template <typename T>
bool WritePod(std::ofstream& out, const T& v)
{
    return WriteBytes(out, &v, sizeof(T));
}

bool ReadBytes(std::ifstream& in, void* data, size_t n)
{
    in.read(reinterpret_cast<char*>(data), static_cast<std::streamsize>(n));
    return static_cast<bool>(in) && static_cast<size_t>(in.gcount()) == n;
}

template <typename T>
bool ReadPod(std::ifstream& in, T& v)
{
    return ReadBytes(in, &v, sizeof(T));
}

bool WriteCapture(std::ofstream& out, const CaptureResult& img, std::wstring* err)
{
    if (img.Empty())
    {
        if (err)
            *err = L"图像为空。";
        return false;
    }
    if (img.bgra.size() > ProjectIO::kMaxImageBytes)
    {
        if (err)
            *err = L"图像超过大小上限。";
        return false;
    }
    const int32_t w = img.width;
    const int32_t h = img.height;
    const int32_t stride = img.stride;
    const int32_t L = img.screenRect.left;
    const int32_t T = img.screenRect.top;
    const int32_t R = img.screenRect.right;
    const int32_t B = img.screenRect.bottom;
    const uint32_t nbytes = static_cast<uint32_t>(img.bgra.size());
    return WritePod(out, w) && WritePod(out, h) && WritePod(out, stride) && WritePod(out, L) &&
           WritePod(out, T) && WritePod(out, R) && WritePod(out, B) && WritePod(out, nbytes) &&
           WriteBytes(out, img.bgra.data(), img.bgra.size());
}

bool ReadCapture(std::ifstream& in, CaptureResult& img, std::wstring* err)
{
    int32_t w = 0, h = 0, stride = 0, L = 0, T = 0, R = 0, B = 0;
    uint32_t nbytes = 0;
    if (!ReadPod(in, w) || !ReadPod(in, h) || !ReadPod(in, stride) || !ReadPod(in, L) ||
        !ReadPod(in, T) || !ReadPod(in, R) || !ReadPod(in, B) || !ReadPod(in, nbytes))
    {
        if (err)
            *err = L"读取图像头失败。";
        return false;
    }
    if (w <= 0 || h <= 0 || nbytes == 0 || nbytes > ProjectIO::kMaxImageBytes)
    {
        if (err)
            *err = L"图像尺寸非法或过大。";
        return false;
    }
    img.width = w;
    img.height = h;
    img.stride = stride;
    img.screenRect = { L, T, R, B };
    img.bgra.resize(nbytes);
    if (!ReadBytes(in, img.bgra.data(), nbytes))
    {
        if (err)
            *err = L"读取图像像素失败。";
        return false;
    }
    return true;
}

bool WriteShape(std::ofstream& out, const DrawCommand& cmd)
{
    const int32_t kind = static_cast<int32_t>(cmd.kind);
    const float cr = cmd.style.color.r, cg = cmd.style.color.g, cb = cmd.style.color.b,
                ca = cmd.style.color.a;
    const float width = cmd.style.width;
    const float fontSize = cmd.style.fontSize;
    const int32_t mosaic = cmd.style.mosaicSize;
    const uint8_t flags = static_cast<uint8_t>(
        (cmd.style.arrow ? 1 : 0) | (cmd.style.filled ? 2 : 0) | (cmd.style.bold ? 4 : 0) |
        (cmd.style.italic ? 8 : 0) | (cmd.style.outline ? 16 : 0));
    const uint32_t textChars = static_cast<uint32_t>(cmd.text.size());
    const uint32_t ptCount = static_cast<uint32_t>(cmd.pts.size());
    if (!WritePod(out, kind) || !WritePod(out, cr) || !WritePod(out, cg) || !WritePod(out, cb) ||
        !WritePod(out, ca) || !WritePod(out, width) || !WritePod(out, fontSize) ||
        !WritePod(out, mosaic) || !WritePod(out, flags) || !WritePod(out, textChars))
    {
        return false;
    }
    if (textChars &&
        !WriteBytes(out, cmd.text.data(), textChars * sizeof(wchar_t)))
    {
        return false;
    }
    if (!WritePod(out, ptCount))
    {
        return false;
    }
    for (const auto& p : cmd.pts)
    {
        if (!WritePod(out, p.x) || !WritePod(out, p.y))
        {
            return false;
        }
    }
    return true;
}

bool ReadShape(std::ifstream& in, DrawCommand& cmd, std::wstring* err)
{
    int32_t kind = 0;
    float cr = 0, cg = 0, cb = 0, ca = 1, width = 3, fontSize = 20;
    int32_t mosaic = 8;
    uint8_t flags = 0;
    uint32_t textChars = 0;
    uint32_t ptCount = 0;
    if (!ReadPod(in, kind) || !ReadPod(in, cr) || !ReadPod(in, cg) || !ReadPod(in, cb) ||
        !ReadPod(in, ca) || !ReadPod(in, width) || !ReadPod(in, fontSize) || !ReadPod(in, mosaic) ||
        !ReadPod(in, flags) || !ReadPod(in, textChars))
    {
        if (err)
            *err = L"读取图形头失败。";
        return false;
    }
    if (kind <= 0 || kind >= static_cast<int32_t>(ToolKind::Count) || textChars > 1u << 20)
    {
        if (err)
            *err = L"图形数据损坏。";
        return false;
    }
    cmd.kind = static_cast<ToolKind>(kind);
    cmd.style.color = D2D1::ColorF(cr, cg, cb, ca);
    cmd.style.width = width;
    cmd.style.fontSize = fontSize;
    cmd.style.mosaicSize = mosaic;
    cmd.style.arrow = (flags & 1) != 0;
    cmd.style.filled = (flags & 2) != 0;
    cmd.style.bold = (flags & 4) != 0;
    cmd.style.italic = (flags & 8) != 0;
    cmd.style.outline = (flags & 16) != 0;
    cmd.text.assign(textChars, L'\0');
    if (textChars && !ReadBytes(in, cmd.text.data(), textChars * sizeof(wchar_t)))
    {
        if (err)
            *err = L"读取文字失败。";
        return false;
    }
    if (!ReadPod(in, ptCount) || ptCount > 1u << 20)
    {
        if (err)
            *err = L"点列损坏。";
        return false;
    }
    cmd.pts.resize(ptCount);
    for (uint32_t i = 0; i < ptCount; ++i)
    {
        if (!ReadPod(in, cmd.pts[i].x) || !ReadPod(in, cmd.pts[i].y))
        {
            if (err)
                *err = L"读取点失败。";
            return false;
        }
    }
    return true;
}
} // namespace

std::wstring ProjectIO::ProjectsDir()
{
    return JoinPath(AppSettings::SettingsDir(), L"projects");
}

std::wstring ProjectIO::AutosavePath()
{
    return JoinPath(AppSettings::SettingsDir(), L"autosave.scdoc");
}

std::wstring ProjectIO::RecentListPath()
{
    return JoinPath(AppSettings::SettingsDir(), L"recent_projects.ini");
}

bool ProjectIO::Save(const AnnotatorDocument& doc, const std::wstring& path, std::wstring* err)
{
    if (!doc.Editable())
    {
        if (err)
            *err = L"没有可保存的文档。";
        return false;
    }
    const auto dir = path.substr(0, path.find_last_of(L"\\/"));
    if (!dir.empty())
    {
        CreateDirectoryW(dir.c_str(), nullptr);
    }
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        if (err)
            *err = L"无法写入文件。";
        return false;
    }
    const uint32_t magic = kMagic;
    const uint32_t ver = kVersion;
    const uint32_t flags = doc.freeze.Empty() ||
                                   (doc.freeze.width == doc.base.width &&
                                    doc.freeze.height == doc.base.height &&
                                    doc.freeze.bgra == doc.base.bgra)
                               ? 0u
                               : 1u;
    const uint32_t shapeCount = static_cast<uint32_t>(doc.shapes.size());
    if (!WritePod(out, magic) || !WritePod(out, ver) || !WritePod(out, flags))
    {
        if (err)
            *err = L"写入文件头失败。";
        return false;
    }
    if (!WriteCapture(out, doc.base, err))
    {
        return false;
    }
    if (flags & 1u)
    {
        if (!WriteCapture(out, doc.freeze, err))
        {
            return false;
        }
    }
    if (!WritePod(out, shapeCount))
    {
        if (err)
            *err = L"写入图形数量失败。";
        return false;
    }
    for (const auto& sh : doc.shapes)
    {
        if (!WriteShape(out, sh))
        {
            if (err)
                *err = L"写入图形失败。";
            return false;
        }
    }
    return true;
}

bool ProjectIO::Load(const std::wstring& path, AnnotatorDocument& outDoc, std::wstring* err)
{
    std::ifstream in(path, std::ios::binary);
    if (!in)
    {
        if (err)
            *err = L"无法打开文件。";
        return false;
    }
    uint32_t magic = 0, ver = 0, flags = 0;
    if (!ReadPod(in, magic) || !ReadPod(in, ver) || !ReadPod(in, flags) || magic != kMagic)
    {
        if (err)
            *err = L"不是有效的 SnipClone 项目文件。";
        return false;
    }
    if (ver != kVersion)
    {
        if (err)
            *err = L"不支持的项目版本。";
        return false;
    }
    AnnotatorDocument doc;
    if (!ReadCapture(in, doc.base, err))
    {
        return false;
    }
    if (flags & 1u)
    {
        if (!ReadCapture(in, doc.freeze, err))
        {
            return false;
        }
    }
    else
    {
        doc.freeze = doc.base;
    }
    uint32_t shapeCount = 0;
    if (!ReadPod(in, shapeCount) || shapeCount > 10000u)
    {
        if (err)
            *err = L"图形数量非法。";
        return false;
    }
    doc.shapes.resize(shapeCount);
    for (uint32_t i = 0; i < shapeCount; ++i)
    {
        if (!ReadShape(in, doc.shapes[i], err))
        {
            return false;
        }
    }
    outDoc = std::move(doc);
    return true;
}

bool ProjectIO::SaveAutosave(const AnnotatorDocument& doc, std::wstring* err)
{
    CreateDirectoryW(AppSettings::SettingsDir().c_str(), nullptr);
    return Save(doc, AutosavePath(), err);
}

bool ProjectIO::LoadAutosave(AnnotatorDocument& out, std::wstring* err)
{
    return Load(AutosavePath(), out, err);
}

bool ProjectIO::HasAutosave()
{
    const auto path = AutosavePath();
    const DWORD attr = GetFileAttributesW(path.c_str());
    return attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

bool ProjectIO::ClearAutosave()
{
    const auto path = AutosavePath();
    if (!HasAutosave())
    {
        return true;
    }
    return DeleteFileW(path.c_str()) != FALSE;
}

void ProjectIO::RememberRecent(const std::wstring& path)
{
    if (path.empty())
    {
        return;
    }
    auto list = ListRecent();
    list.erase(std::remove(list.begin(), list.end(), path), list.end());
    list.insert(list.begin(), path);
    if (list.size() > kMaxRecent)
    {
        list.resize(kMaxRecent);
    }
    CreateDirectoryW(AppSettings::SettingsDir().c_str(), nullptr);
    const auto ini = RecentListPath();
    WritePrivateProfileStringW(L"Recent", L"Count", std::to_wstring(list.size()).c_str(), ini.c_str());
    for (size_t i = 0; i < list.size(); ++i)
    {
        wchar_t key[32]{};
        swprintf_s(key, L"%u", static_cast<unsigned>(i));
        WritePrivateProfileStringW(L"Recent", key, list[i].c_str(), ini.c_str());
    }
}

std::vector<std::wstring> ProjectIO::ListRecent()
{
    std::vector<std::wstring> list;
    const auto ini = RecentListPath();
    const int count = GetPrivateProfileIntW(L"Recent", L"Count", 0, ini.c_str());
    for (int i = 0; i < count && i < static_cast<int>(kMaxRecent); ++i)
    {
        wchar_t key[32]{};
        swprintf_s(key, L"%d", i);
        wchar_t buf[MAX_PATH]{};
        GetPrivateProfileStringW(L"Recent", key, L"", buf, MAX_PATH, ini.c_str());
        if (buf[0])
        {
            list.emplace_back(buf);
        }
    }
    return list;
}

bool ProjectIO::ClearRecent()
{
    const auto ini = RecentListPath();
    return DeleteFileW(ini.c_str()) != FALSE || GetLastError() == ERROR_FILE_NOT_FOUND;
}

bool ProjectIO::BrowseSavePath(HWND owner, std::wstring& pathOut)
{
    wchar_t file[MAX_PATH] = L"untitled.scdoc";
    OPENFILENAMEW ofn{};
    ofn.lStructSize = sizeof(ofn);
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = L"SnipClone 项目 (*.scdoc)\0*.scdoc\0所有文件 (*.*)\0*.*\0";
    ofn.lpstrFile = file;
    ofn.nMaxFile = MAX_PATH;
    ofn.lpstrDefExt = L"scdoc";
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
    CreateDirectoryW(ProjectsDir().c_str(), nullptr);
    ofn.lpstrInitialDir = ProjectsDir().c_str();
    if (!GetSaveFileNameW(&ofn))
    {
        return false;
    }
    pathOut = file;
    return true;
}

bool ProjectIO::BrowseOpenPath(HWND owner, std::wstring& pathOut)
{
    wchar_t file[MAX_PATH]{};
    OPENFILENAMEW ofn{};
    ofn.lStructSize = sizeof(ofn);
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = L"SnipClone 项目 (*.scdoc)\0*.scdoc\0所有文件 (*.*)\0*.*\0";
    ofn.lpstrFile = file;
    ofn.nMaxFile = MAX_PATH;
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    ofn.lpstrInitialDir = ProjectsDir().c_str();
    if (!GetOpenFileNameW(&ofn))
    {
        return false;
    }
    pathOut = file;
    return true;
}

} // namespace snip
