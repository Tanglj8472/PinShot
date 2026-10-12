//==============================================================================
// SnipClone — WIC PNG encode / clipboard write
//==============================================================================
#include "ImageUtil.h"

#include <commdlg.h>
#include <wincodec.h>

#include <vector>

#pragma comment(lib, "windowscodecs.lib")
#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "comdlg32.lib")

namespace snip
{
namespace
{
bool InitCom()
{
    HRESULT hr = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    return SUCCEEDED(hr) || hr == RPC_E_CHANGED_MODE;
}

std::vector<std::uint8_t> ToBottomUpBgrDib(const CaptureResult& image)
{
    // CF_DIB expects BITMAPINFOHEADER + bottom-up BGR(A) pixels.
    const size_t headerSize = sizeof(BITMAPINFOHEADER);
    const size_t pixelBytes = static_cast<size_t>(image.stride) * static_cast<size_t>(image.height);
    std::vector<std::uint8_t> dib(headerSize + pixelBytes);

    auto* bi = reinterpret_cast<BITMAPINFOHEADER*>(dib.data());
    bi->biSize = sizeof(BITMAPINFOHEADER);
    bi->biWidth = image.width;
    bi->biHeight = image.height; // bottom-up
    bi->biPlanes = 1;
    bi->biBitCount = 32;
    bi->biCompression = BI_RGB;
    bi->biSizeImage = static_cast<DWORD>(pixelBytes);

    auto* dst = dib.data() + headerSize;
    for (int y = 0; y < image.height; ++y)
    {
        const int srcY = image.height - 1 - y;
        memcpy(
            dst + static_cast<size_t>(y) * image.stride,
            image.bgra.data() + static_cast<size_t>(srcY) * image.stride,
            static_cast<size_t>(image.stride));
    }
    return dib;
}
} // namespace

bool ImageUtil::SavePng(const CaptureResult& image, const std::wstring& path)
{
    if (image.Empty() || path.empty())
    {
        return false;
    }
    InitCom();

    IWICImagingFactory* factory = nullptr;
    if (FAILED(CoCreateInstance(
            CLSID_WICImagingFactory,
            nullptr,
            CLSCTX_INPROC_SERVER,
            IID_PPV_ARGS(&factory))))
    {
        return false;
    }

    IWICStream* stream = nullptr;
    IWICBitmapEncoder* encoder = nullptr;
    IWICBitmapFrameEncode* frame = nullptr;
    IPropertyBag* props = nullptr;
    bool ok = false;

    do
    {
        if (FAILED(factory->CreateStream(&stream)))
            break;
        if (FAILED(stream->InitializeFromFilename(path.c_str(), GENERIC_WRITE)))
            break;
        if (FAILED(factory->CreateEncoder(GUID_ContainerFormatPng, nullptr, &encoder)))
            break;
        if (FAILED(encoder->Initialize(stream, WICBitmapEncoderNoCache)))
            break;
        if (FAILED(encoder->CreateNewFrame(&frame, &props)))
            break;
        if (FAILED(frame->Initialize(props)))
            break;
        if (FAILED(frame->SetSize(static_cast<UINT>(image.width), static_cast<UINT>(image.height))))
            break;

        WICPixelFormatGUID format = GUID_WICPixelFormat32bppBGRA;
        if (FAILED(frame->SetPixelFormat(&format)))
            break;
        if (FAILED(frame->WritePixels(
                static_cast<UINT>(image.height),
                static_cast<UINT>(image.stride),
                static_cast<UINT>(image.bgra.size()),
                const_cast<BYTE*>(image.bgra.data()))))
            break;
        if (FAILED(frame->Commit()))
            break;
        if (FAILED(encoder->Commit()))
            break;
        ok = true;
    } while (false);

    if (props)
        props->Release();
    if (frame)
        frame->Release();
    if (encoder)
        encoder->Release();
    if (stream)
        stream->Release();
    factory->Release();
    return ok;
}

bool ImageUtil::CopyToClipboard(const CaptureResult& image)
{
    if (image.Empty())
    {
        return false;
    }

    auto dib = ToBottomUpBgrDib(image);
    HGLOBAL mem = GlobalAlloc(GMEM_MOVEABLE, dib.size());
    if (!mem)
    {
        return false;
    }
    void* ptr = GlobalLock(mem);
    if (!ptr)
    {
        GlobalFree(mem);
        return false;
    }
    memcpy(ptr, dib.data(), dib.size());
    GlobalUnlock(mem);

    if (!OpenClipboard(nullptr))
    {
        GlobalFree(mem);
        return false;
    }
    EmptyClipboard();
    if (!SetClipboardData(CF_DIB, mem))
    {
        CloseClipboard();
        GlobalFree(mem);
        return false;
    }
    CloseClipboard();
    // Clipboard owns mem now.
    return true;
}

std::wstring ImageUtil::BrowseSavePng(HWND owner)
{
    InitCom();

    wchar_t path[MAX_PATH]{};
    OPENFILENAMEW ofn{};
    ofn.lStructSize = sizeof(ofn);
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = L"PNG (*.png)\0*.png\0All Files (*.*)\0*.*\0";
    ofn.lpstrFile = path;
    ofn.nMaxFile = MAX_PATH;
    ofn.lpstrDefExt = L"png";
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
    ofn.lpstrTitle = L"Save snip";
    if (GetSaveFileNameW(&ofn))
    {
        return path;
    }
    return {};
}

} // namespace snip
