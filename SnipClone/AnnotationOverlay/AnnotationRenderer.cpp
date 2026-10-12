//==============================================================================
// SnipClone — D2D annotation renderer
//==============================================================================
#include "AnnotationRenderer.h"

#include <cmath>
#include <vector>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "windowscodecs.lib")

namespace snip
{

AnnotationRenderer::~AnnotationRenderer()
{
    Shutdown();
}

bool AnnotationRenderer::Initialize()
{
    if (m_factory)
    {
        return true;
    }
    if (FAILED(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &m_factory)))
    {
        return false;
    }
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    if (FAILED(CoCreateInstance(
            CLSID_WICImagingFactory,
            nullptr,
            CLSCTX_INPROC_SERVER,
            IID_PPV_ARGS(&m_wic))))
    {
        return false;
    }
    return true;
}

void AnnotationRenderer::Shutdown()
{
    if (m_brush)
    {
        m_brush->Release();
        m_brush = nullptr;
    }
    if (m_wic)
    {
        m_wic->Release();
        m_wic = nullptr;
    }
    if (m_factory)
    {
        m_factory->Release();
        m_factory = nullptr;
    }
}

void AnnotationRenderer::EnsureBrushes(ID2D1RenderTarget* rt)
{
    if (!m_brush)
    {
        rt->CreateSolidColorBrush(D2D1::ColorF(1, 1, 1), &m_brush);
    }
}

ID2D1Bitmap* AnnotationRenderer::CreateBitmapFromCapture(ID2D1RenderTarget* rt, const CaptureResult& image)
{
    if (!rt || image.Empty())
    {
        return nullptr;
    }

    D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    // Premultiply for layered/D2D.
    std::vector<std::uint8_t> premul(image.bgra.size());
    for (int y = 0; y < image.height; ++y)
    {
        const auto* src = image.bgra.data() + static_cast<size_t>(y) * image.stride;
        auto* dst = premul.data() + static_cast<size_t>(y) * image.stride;
        for (int x = 0; x < image.width; ++x)
        {
            const std::uint8_t b = src[x * 4 + 0];
            const std::uint8_t g = src[x * 4 + 1];
            const std::uint8_t r = src[x * 4 + 2];
            const std::uint8_t a = src[x * 4 + 3];
            dst[x * 4 + 0] = static_cast<std::uint8_t>(b * a / 255);
            dst[x * 4 + 1] = static_cast<std::uint8_t>(g * a / 255);
            dst[x * 4 + 2] = static_cast<std::uint8_t>(r * a / 255);
            dst[x * 4 + 3] = a;
        }
    }

    ID2D1Bitmap* bmp = nullptr;
    rt->CreateBitmap(
        D2D1::SizeU(static_cast<UINT32>(image.width), static_cast<UINT32>(image.height)),
        premul.data(),
        static_cast<UINT32>(image.stride),
        props,
        &bmp);
    return bmp;
}

void AnnotationRenderer::DrawArrowHead(
    ID2D1RenderTarget* rt,
    D2D1_POINT_2F from,
    D2D1_POINT_2F to,
    const DrawStyle& style)
{
    const float dx = to.x - from.x;
    const float dy = to.y - from.y;
    const float len = std::sqrt(dx * dx + dy * dy);
    if (len < 1.f)
    {
        return;
    }
    const float ux = dx / len;
    const float uy = dy / len;
    const float head = (std::max)(10.f, style.width * 4.f);
    const float px = -uy;
    const float py = ux;

    D2D1_POINT_2F p1{ to.x - ux * head + px * head * 0.45f, to.y - uy * head + py * head * 0.45f };
    D2D1_POINT_2F p2{ to.x - ux * head - px * head * 0.45f, to.y - uy * head - py * head * 0.45f };

    m_brush->SetColor(style.color);
    rt->DrawLine(to, p1, m_brush, style.width, nullptr);
    rt->DrawLine(to, p2, m_brush, style.width, nullptr);
}

void AnnotationRenderer::DrawCommandOn(ID2D1RenderTarget* rt, const DrawCommand& cmd)
{
    if (cmd.pts.empty() || !m_brush)
    {
        return;
    }
    m_brush->SetColor(cmd.style.color);
    rt->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

    switch (cmd.kind)
    {
    case ToolKind::Pen:
        for (size_t i = 1; i < cmd.pts.size(); ++i)
        {
            rt->DrawLine(cmd.pts[i - 1], cmd.pts[i], m_brush, cmd.style.width);
        }
        break;
    case ToolKind::Rect:
        if (cmd.pts.size() >= 2)
        {
            D2D1_RECT_F r = D2D1::RectF(cmd.pts[0].x, cmd.pts[0].y, cmd.pts[1].x, cmd.pts[1].y);
            rt->DrawRectangle(r, m_brush, cmd.style.width);
        }
        break;
    case ToolKind::Arrow:
        if (cmd.pts.size() >= 2)
        {
            rt->DrawLine(cmd.pts[0], cmd.pts[1], m_brush, cmd.style.width);
            if (cmd.style.arrow)
            {
                DrawArrowHead(rt, cmd.pts[0], cmd.pts[1], cmd.style);
            }
        }
        break;
    default:
        break;
    }
}

void AnnotationRenderer::DrawScene(
    ID2D1RenderTarget* rt,
    ID2D1Bitmap* baseBitmap,
    const DrawStack& stack)
{
    EnsureBrushes(rt);
    if (baseBitmap)
    {
        const auto sz = rt->GetSize();
        rt->DrawBitmap(baseBitmap, D2D1::RectF(0, 0, sz.width, sz.height));
    }
    for (const auto& cmd : stack.Shapes())
    {
        DrawCommandOn(rt, cmd);
    }
    if (const auto* live = stack.Live())
    {
        DrawCommandOn(rt, *live);
    }
}

CaptureResult AnnotationRenderer::Compose(const CaptureResult& base, const DrawStack& stack)
{
    CaptureResult out = base;
    if (base.Empty() || !Initialize())
    {
        return out;
    }

    // Render to WIC bitmap → copy pixels out.
    IWICBitmap* wicBmp = nullptr;
    if (FAILED(m_wic->CreateBitmap(
            static_cast<UINT>(base.width),
            static_cast<UINT>(base.height),
            GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnLoad,
            &wicBmp)))
    {
        return out;
    }

    D2D1_RENDER_TARGET_PROPERTIES props = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    ID2D1RenderTarget* rt = nullptr;
    if (FAILED(m_factory->CreateWicBitmapRenderTarget(wicBmp, props, &rt)))
    {
        wicBmp->Release();
        return out;
    }

    ID2D1Bitmap* baseBmp = CreateBitmapFromCapture(rt, base);
    rt->BeginDraw();
    rt->Clear(D2D1::ColorF(0, 0, 0, 0));
    // Temporary stack without live for export (caller should Finish first).
    DrawScene(rt, baseBmp, stack);
    rt->EndDraw();

    if (baseBmp)
    {
        baseBmp->Release();
    }
    rt->Release();

    const UINT stride = static_cast<UINT>(base.stride);
    const UINT bufSize = stride * static_cast<UINT>(base.height);
    out.bgra.resize(bufSize);
    wicBmp->CopyPixels(nullptr, stride, bufSize, out.bgra.data());

    // Convert premultiplied → straight for clipboard/PNG consistency.
    for (int y = 0; y < out.height; ++y)
    {
        auto* row = out.bgra.data() + static_cast<size_t>(y) * out.stride;
        for (int x = 0; x < out.width; ++x)
        {
            const std::uint8_t a = row[x * 4 + 3];
            if (a == 0)
            {
                row[x * 4 + 0] = row[x * 4 + 1] = row[x * 4 + 2] = 0;
            }
            else if (a < 255)
            {
                row[x * 4 + 0] = static_cast<std::uint8_t>(row[x * 4 + 0] * 255 / a);
                row[x * 4 + 1] = static_cast<std::uint8_t>(row[x * 4 + 1] * 255 / a);
                row[x * 4 + 2] = static_cast<std::uint8_t>(row[x * 4 + 2] * 255 / a);
            }
            row[x * 4 + 3] = 255; // snip exports opaque
        }
    }

    wicBmp->Release();
    if (m_brush)
    {
        m_brush->Release();
        m_brush = nullptr;
    }
    return out;
}

} // namespace snip
