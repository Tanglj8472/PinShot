//==============================================================================
// SnipClone — DirectWrite text metrics shared by hit-test / chrome / render
//==============================================================================
#include "TextLayout.h"

#include <dwrite.h>

#include <algorithm>

#pragma comment(lib, "dwrite.lib")

namespace snip
{
namespace
{
IDWriteFactory* Factory()
{
    static IDWriteFactory* factory = nullptr;
    if (!factory)
    {
        DWriteCreateFactory(
            DWRITE_FACTORY_TYPE_SHARED,
            __uuidof(IDWriteFactory),
            reinterpret_cast<IUnknown**>(&factory));
    }
    return factory;
}
} // namespace

TextExtent MeasureAnnotationText(const std::wstring& text, const DrawStyle& style)
{
    TextExtent ext;
    const float fontSize = (std::max)(8.f, style.fontSize);
    // Fallback estimate when DWrite unavailable (headless edge cases).
    ext.width = (std::max)(40.f, fontSize * 1.05f * static_cast<float>(text.size() + 1));
    ext.height = fontSize * 1.5f;

    IDWriteFactory* factory = Factory();
    if (!factory || text.empty())
    {
        return ext;
    }

    IDWriteTextFormat* format = nullptr;
    if (FAILED(factory->CreateTextFormat(
            L"Segoe UI",
            nullptr,
            style.bold ? DWRITE_FONT_WEIGHT_BOLD : DWRITE_FONT_WEIGHT_NORMAL,
            style.italic ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            fontSize,
            L"zh-cn",
            &format)) ||
        !format)
    {
        return ext;
    }

    IDWriteTextLayout* layout = nullptr;
    const FLOAT maxW = 8000.f;
    const FLOAT maxH = fontSize * 64.f;
    if (SUCCEEDED(factory->CreateTextLayout(
            text.c_str(),
            static_cast<UINT32>(text.size()),
            format,
            maxW,
            maxH,
            &layout)) &&
        layout)
    {
        DWRITE_TEXT_METRICS metrics{};
        if (SUCCEEDED(layout->GetMetrics(&metrics)))
        {
            ext.width = (std::max)(12.f, metrics.widthIncludingTrailingWhitespace + 2.f);
            ext.height = (std::max)(fontSize, metrics.height + 2.f);
        }
        layout->Release();
    }
    format->Release();
    return ext;
}

} // namespace snip
