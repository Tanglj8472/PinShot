//==============================================================================
// SnipClone — D2D annotation renderer（文字 / 马赛克对齐 PinShot 效果思路）
//==============================================================================
#include "AnnotationRenderer.h"

#include <algorithm>
#include <cmath>
#include <vector>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "dwrite.lib")
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
    if (FAILED(DWriteCreateFactory(
            DWRITE_FACTORY_TYPE_SHARED,
            __uuidof(IDWriteFactory),
            reinterpret_cast<IUnknown**>(&m_dwrite))))
    {
        return false;
    }
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
    if (m_dwrite)
    {
        m_dwrite->Release();
        m_dwrite = nullptr;
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

    // Same path as SelectionOverlay::CreatePremulBitmap (known-good on DC/WIC RTs).
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
            const std::uint8_t a = src[x * 4 + 3] ? src[x * 4 + 3] : 255;
            dst[x * 4 + 0] = static_cast<std::uint8_t>(b * a / 255);
            dst[x * 4 + 1] = static_cast<std::uint8_t>(g * a / 255);
            dst[x * 4 + 2] = static_cast<std::uint8_t>(r * a / 255);
            dst[x * 4 + 3] = a;
        }
    }

    D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    ID2D1Bitmap* bmp = nullptr;
    if (FAILED(rt->CreateBitmap(
            D2D1::SizeU(static_cast<UINT32>(image.width), static_cast<UINT32>(image.height)),
            premul.data(),
            static_cast<UINT32>(image.stride),
            props,
            &bmp)) ||
        !bmp)
    {
        return nullptr;
    }
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

void AnnotationRenderer::DrawTextCmd(ID2D1RenderTarget* rt, const DrawCommand& cmd)
{
    if (!m_dwrite || cmd.pts.empty() || cmd.text.empty())
    {
        return;
    }

    IDWriteTextFormat* format = nullptr;
    if (FAILED(m_dwrite->CreateTextFormat(
            L"Segoe UI",
            nullptr,
            cmd.style.bold ? DWRITE_FONT_WEIGHT_BOLD : DWRITE_FONT_WEIGHT_NORMAL,
            cmd.style.italic ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            cmd.style.fontSize,
            L"zh-cn",
            &format)))
    {
        return;
    }

    const float x = cmd.pts[0].x;
    const float y = cmd.pts[0].y;
    const float w = 4000.f;
    const float h = cmd.style.fontSize * 6.f;
    D2D1_RECT_F layout = D2D1::RectF(x, y, x + w, y + h);

    // PinShot/Snipaste outline: light text → black halo, dark text → white halo.
    if (cmd.style.outline)
    {
        const float lum = 0.2126f * cmd.style.color.r + 0.7152f * cmd.style.color.g + 0.0722f * cmd.style.color.b;
        const float halo = (std::max)(2.f, cmd.style.fontSize / 7.f);
        m_brush->SetColor(lum > 0.7f ? D2D1::ColorF(0, 0, 0, 1.f) : D2D1::ColorF(1, 1, 1, 1.f));
        const int rad = (std::max)(1, static_cast<int>(std::lround(halo)));
        for (int ox = -rad; ox <= rad; ++ox)
        {
            for (int oy = -rad; oy <= rad; ++oy)
            {
                if (ox == 0 && oy == 0)
                {
                    continue;
                }
                if (ox * ox + oy * oy > rad * rad)
                {
                    continue;
                }
                D2D1_RECT_F r = D2D1::RectF(
                    layout.left + static_cast<float>(ox),
                    layout.top + static_cast<float>(oy),
                    layout.right + static_cast<float>(ox),
                    layout.bottom + static_cast<float>(oy));
                rt->DrawText(cmd.text.c_str(), static_cast<UINT32>(cmd.text.size()), format, r, m_brush);
            }
        }
    }

    m_brush->SetColor(cmd.style.color);
    rt->DrawText(cmd.text.c_str(), static_cast<UINT32>(cmd.text.size()), format, layout, m_brush);
    format->Release();
}

void AnnotationRenderer::DrawEraser(
    ID2D1RenderTarget* rt,
    ID2D1Bitmap* baseBitmap,
    const CaptureResult* basePixels,
    const DrawCommand& cmd)
{
    if (cmd.pts.empty() || !basePixels || basePixels->Empty())
    {
        UNREFERENCED_PARAMETER(baseBitmap);
        return;
    }

    // Brush eraser: stamp original freeze pixels along the stroke (PinShot restore).
    const int rad = (std::max)(2, static_cast<int>(std::lround(cmd.style.width * 0.5f)));
    const int dia = rad * 2 + 1;
    std::vector<std::uint8_t> stamp(static_cast<size_t>(dia * dia * 4));

    D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));

    auto stampAt = [&](float fx, float fy) {
        const int cx = static_cast<int>(std::lround(fx));
        const int cy = static_cast<int>(std::lround(fy));
        for (int y = 0; y < dia; ++y)
        {
            for (int x = 0; x < dia; ++x)
            {
                const int dx = x - rad;
                const int dy = y - rad;
                auto* dst = stamp.data() + static_cast<size_t>((y * dia + x) * 4);
                if (dx * dx + dy * dy > rad * rad)
                {
                    dst[0] = dst[1] = dst[2] = dst[3] = 0;
                    continue;
                }
                const int sx = cx + dx;
                const int sy = cy + dy;
                if (sx < 0 || sy < 0 || sx >= basePixels->width || sy >= basePixels->height)
                {
                    dst[0] = dst[1] = dst[2] = dst[3] = 0;
                    continue;
                }
                const auto* src = basePixels->bgra.data() +
                                  static_cast<size_t>(sy) * basePixels->stride +
                                  static_cast<size_t>(sx) * 4;
                dst[0] = src[0];
                dst[1] = src[1];
                dst[2] = src[2];
                dst[3] = 255;
            }
        }
        ID2D1Bitmap* bmp = nullptr;
        rt->CreateBitmap(
            D2D1::SizeU(static_cast<UINT32>(dia), static_cast<UINT32>(dia)),
            stamp.data(),
            static_cast<UINT32>(dia * 4),
            props,
            &bmp);
        if (bmp)
        {
            const float l = static_cast<float>(cx - rad);
            const float t = static_cast<float>(cy - rad);
            rt->DrawBitmap(bmp, D2D1::RectF(l, t, l + dia, t + dia));
            bmp->Release();
        }
    };

    // Subsample dense strokes for performance.
    float distAccum = 0.f;
    const float step = (std::max)(1.f, cmd.style.width * 0.35f);
    stampAt(cmd.pts[0].x, cmd.pts[0].y);
    for (size_t i = 1; i < cmd.pts.size(); ++i)
    {
        const float dx = cmd.pts[i].x - cmd.pts[i - 1].x;
        const float dy = cmd.pts[i].y - cmd.pts[i - 1].y;
        distAccum += std::sqrt(dx * dx + dy * dy);
        if (distAccum >= step || i + 1 == cmd.pts.size())
        {
            stampAt(cmd.pts[i].x, cmd.pts[i].y);
            distAccum = 0.f;
        }
    }
}

void AnnotationRenderer::DrawMosaic(
    ID2D1RenderTarget* rt,
    ID2D1Bitmap* baseBitmap,
    const CaptureResult* basePixels,
    const DrawCommand& cmd)
{
    if (cmd.pts.size() < 2 || !baseBitmap)
    {
        return;
    }

    const float l = (std::min)(cmd.pts[0].x, cmd.pts[1].x);
    const float t = (std::min)(cmd.pts[0].y, cmd.pts[1].y);
    const float r = (std::max)(cmd.pts[0].x, cmd.pts[1].x);
    const float b = (std::max)(cmd.pts[0].y, cmd.pts[1].y);
    const float rw = r - l;
    const float rh = b - t;
    if (rw < 2.f || rh < 2.f)
    {
        return;
    }

    const int block = (std::max)(2, cmd.style.mosaicSize);
    // PinShot-style: scale down then nearest-neighbor up via D2D bitmap draw.
    const UINT smallW = (std::max)(1u, static_cast<UINT>(rw / block));
    const UINT smallH = (std::max)(1u, static_cast<UINT>(rh / block));

    // Prefer sampling from CPU base for correct source rect; fall back to full bitmap stretch.
    if (basePixels && !basePixels->Empty())
    {
        const int x0 = (std::max)(0, static_cast<int>(l));
        const int y0 = (std::max)(0, static_cast<int>(t));
        const int x1 = (std::min)(basePixels->width, static_cast<int>(r));
        const int y1 = (std::min)(basePixels->height, static_cast<int>(b));
        const int cw = x1 - x0;
        const int ch = y1 - y0;
        if (cw <= 0 || ch <= 0)
        {
            return;
        }

        const int sw = (std::max)(1, cw / block);
        const int sh = (std::max)(1, ch / block);
        // NOTE: do not name this `small` — Windows rpcndr.h does `#define small char`.
        std::vector<std::uint8_t> mosaicLo(static_cast<size_t>(sw * sh * 4));
        for (int sy = 0; sy < sh; ++sy)
        {
            const int srcY = y0 + sy * ch / sh;
            for (int sx = 0; sx < sw; ++sx)
            {
                const int srcX = x0 + sx * cw / sw;
                const auto* srcPx = basePixels->bgra.data() +
                                    static_cast<size_t>(srcY) * basePixels->stride +
                                    static_cast<size_t>(srcX) * 4;
                auto* dstPx = mosaicLo.data() + static_cast<size_t>((sy * sw + sx) * 4);
                dstPx[0] = srcPx[0];
                dstPx[1] = srcPx[1];
                dstPx[2] = srcPx[2];
                dstPx[3] = 255;
            }
        }

        // Upscale on CPU with nearest neighbor into region-sized buffer.
        std::vector<std::uint8_t> mosaicHi(static_cast<size_t>(cw * ch * 4));
        for (int y = 0; y < ch; ++y)
        {
            const int sy = y * sh / ch;
            for (int x = 0; x < cw; ++x)
            {
                const int sx = x * sw / cw;
                const auto* srcPx = mosaicLo.data() + static_cast<size_t>((sy * sw + sx) * 4);
                auto* dstPx = mosaicHi.data() + static_cast<size_t>((y * cw + x) * 4);
                dstPx[0] = srcPx[0];
                dstPx[1] = srcPx[1];
                dstPx[2] = srcPx[2];
                dstPx[3] = 255;
            }
        }

        D2D1_BITMAP_PROPERTIES props = D2D1::BitmapProperties(
            D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
        ID2D1Bitmap* mosa = nullptr;
        // Premultiply (opaque).
        rt->CreateBitmap(
            D2D1::SizeU(static_cast<UINT32>(cw), static_cast<UINT32>(ch)),
            mosaicHi.data(),
            static_cast<UINT32>(cw * 4),
            props,
            &mosa);
        if (mosa)
        {
            rt->DrawBitmap(
                mosa,
                D2D1::RectF(static_cast<float>(x0), static_cast<float>(y0),
                            static_cast<float>(x1), static_cast<float>(y1)),
                1.f,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
            mosa->Release();
        }
        return;
    }

    // Fallback: draw source sub-rect via full bitmap with nearest neighbor into dest.
    UNREFERENCED_PARAMETER(smallW);
    UNREFERENCED_PARAMETER(smallH);
    rt->DrawBitmap(
        baseBitmap,
        D2D1::RectF(l, t, r, b),
        1.f,
        D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
        D2D1::RectF(l, t, r, b));
}

void AnnotationRenderer::DrawCommandOn(
    ID2D1RenderTarget* rt,
    ID2D1Bitmap* baseBitmap,
    const CaptureResult* basePixels,
    const DrawCommand& cmd,
    bool preview)
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
            if (cmd.style.filled)
            {
                rt->FillRectangle(r, m_brush);
            }
            else
            {
                rt->DrawRectangle(r, m_brush, cmd.style.width);
            }
        }
        break;
    case ToolKind::Ellipse:
        if (cmd.pts.size() >= 2)
        {
            D2D1_ELLIPSE e = D2D1::Ellipse(
                D2D1::Point2F((cmd.pts[0].x + cmd.pts[1].x) * 0.5f, (cmd.pts[0].y + cmd.pts[1].y) * 0.5f),
                std::fabs(cmd.pts[1].x - cmd.pts[0].x) * 0.5f,
                std::fabs(cmd.pts[1].y - cmd.pts[0].y) * 0.5f);
            if (cmd.style.filled)
            {
                rt->FillEllipse(e, m_brush);
            }
            else
            {
                rt->DrawEllipse(e, m_brush, cmd.style.width);
            }
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
    case ToolKind::Text:
        DrawTextCmd(rt, cmd);
        break;
    case ToolKind::Mosaic:
        DrawMosaic(rt, baseBitmap, basePixels, cmd);
        if (preview && cmd.pts.size() >= 2)
        {
            m_brush->SetColor(D2D1::ColorF(1, 1, 1, 0.6f));
            rt->DrawRectangle(
                D2D1::RectF(cmd.pts[0].x, cmd.pts[0].y, cmd.pts[1].x, cmd.pts[1].y),
                m_brush,
                1.f);
        }
        break;
    case ToolKind::Eraser:
        DrawEraser(rt, baseBitmap, basePixels, cmd);
        if (preview)
        {
            m_brush->SetColor(D2D1::ColorF(1, 1, 1, 0.35f));
            const float rad = (std::max)(2.f, cmd.style.width * 0.5f);
            for (const auto& p : cmd.pts)
            {
                rt->FillEllipse(D2D1::Ellipse(p, rad, rad), m_brush);
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
    const CaptureResult* basePixels,
    const DrawStack& stack)
{
    EnsureBrushes(rt);
    if (baseBitmap)
    {
        // Prefer pixel size from capture; GetSize() is DIP and can mismatch if DPI unset.
        float dw = 0.f;
        float dh = 0.f;
        if (basePixels && !basePixels->Empty())
        {
            dw = static_cast<float>(basePixels->width);
            dh = static_cast<float>(basePixels->height);
        }
        else
        {
            const auto px = baseBitmap->GetPixelSize();
            dw = static_cast<float>(px.width);
            dh = static_cast<float>(px.height);
        }
        rt->DrawBitmap(baseBitmap, D2D1::RectF(0, 0, dw, dh));
    }
    for (const auto& cmd : stack.Shapes())
    {
        DrawCommandOn(rt, baseBitmap, basePixels, cmd, false);
    }
    if (const auto* live = stack.Live())
    {
        DrawCommandOn(rt, baseBitmap, basePixels, *live, true);
    }
    // Selection outline (PinShot select tool).
    if (stack.SelectedIndex() >= 0)
    {
        if (const auto* sh = stack.ShapeAt(stack.SelectedIndex()))
        {
            if (!sh->pts.empty())
            {
                float minX = sh->pts[0].x, maxX = sh->pts[0].x;
                float minY = sh->pts[0].y, maxY = sh->pts[0].y;
                for (const auto& p : sh->pts)
                {
                    minX = (std::min)(minX, p.x);
                    maxX = (std::max)(maxX, p.x);
                    minY = (std::min)(minY, p.y);
                    maxY = (std::max)(maxY, p.y);
                }
                if (sh->kind == ToolKind::Text)
                {
                    maxX = minX + (std::max)(40.f, sh->style.fontSize * 0.6f * static_cast<float>(sh->text.size() + 1));
                    maxY = minY + sh->style.fontSize * 1.4f;
                }
                const float pad = sh->style.width + 3.f;
                m_brush->SetColor(D2D1::ColorF(0.12f, 0.56f, 1.f, 0.95f));
                rt->DrawRectangle(
                    D2D1::RectF(minX - pad, minY - pad, maxX + pad, maxY + pad),
                    m_brush,
                    1.5f);
            }
        }
    }
}

CaptureResult AnnotationRenderer::Compose(const CaptureResult& base, const DrawStack& stack)
{
    CaptureResult out = base;
    if (base.Empty() || !Initialize())
    {
        return out;
    }

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
    if (m_brush)
    {
        m_brush->Release();
        m_brush = nullptr;
    }
    rt->BeginDraw();
    rt->Clear(D2D1::ColorF(0, 0, 0, 0));
    DrawScene(rt, baseBmp, &base, stack);
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

    // Flatten to opaque BGRA for layered ULW / GDI. Transparent holes must fall
    // back to the freeze crop — NOT opaque black (that caused the "blank snip").
    for (int y = 0; y < out.height; ++y)
    {
        auto* row = out.bgra.data() + static_cast<size_t>(y) * out.stride;
        const auto* baseRow = base.bgra.data() + static_cast<size_t>(y) * base.stride;
        for (int x = 0; x < out.width; ++x)
        {
            const std::uint8_t a = row[x * 4 + 3];
            if (a == 0)
            {
                row[x * 4 + 0] = baseRow[x * 4 + 0];
                row[x * 4 + 1] = baseRow[x * 4 + 1];
                row[x * 4 + 2] = baseRow[x * 4 + 2];
            }
            else if (a < 255)
            {
                row[x * 4 + 0] = static_cast<std::uint8_t>(row[x * 4 + 0] * 255 / a);
                row[x * 4 + 1] = static_cast<std::uint8_t>(row[x * 4 + 1] * 255 / a);
                row[x * 4 + 2] = static_cast<std::uint8_t>(row[x * 4 + 2] * 255 / a);
            }
            row[x * 4 + 3] = 255;
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
