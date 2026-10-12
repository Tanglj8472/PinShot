//==============================================================================
// SnipClone — D2D annotation renderer
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "DrawCommand.h"
#include "DrawStack.h"

#include <d2d1.h>
#include <dwrite.h>
#include <wincodec.h>

namespace snip
{

class AnnotationRenderer
{
public:
    AnnotationRenderer() = default;
    ~AnnotationRenderer();

    AnnotationRenderer(const AnnotationRenderer&) = delete;
    AnnotationRenderer& operator=(const AnnotationRenderer&) = delete;

    bool Initialize();
    void Shutdown();

    // includeSelectionChrome: blue select outline (preview only — never export).
    // includeLive: in-progress stroke / preview overlays (export after Finish() is empty).
    void DrawScene(
        ID2D1RenderTarget* rt,
        ID2D1Bitmap* baseBitmap,
        const CaptureResult* basePixels,
        const DrawStack& stack,
        bool includeSelectionChrome = true,
        bool includeLive = true);

    // On any WIC/D2D failure returns Empty() — never a silent base copy (A02).
    // ok: optional; set false on failure, true on success.
    // reuseSurface: keep WIC/RT/base bitmap between preview frames (A11). Export should pass false.
    CaptureResult Compose(
        const CaptureResult& base,
        const DrawStack& stack,
        bool includeSelectionChrome = true,
        bool includeLive = true,
        bool* ok = nullptr,
        bool reuseSurface = false);

    ID2D1Bitmap* CreateBitmapFromCapture(ID2D1RenderTarget* rt, const CaptureResult& image);

    ID2D1Factory* Factory() const { return m_factory; }
    void ReleaseComposeCache();

private:
    bool EnsureComposeCache(const CaptureResult& base);
    void EnsureBrushes(ID2D1RenderTarget* rt);
    void DrawCommandOn(
        ID2D1RenderTarget* rt,
        ID2D1Bitmap* baseBitmap,
        const CaptureResult* basePixels,
        const DrawCommand& cmd,
        bool preview);
    void DrawArrowHead(ID2D1RenderTarget* rt, D2D1_POINT_2F from, D2D1_POINT_2F to, const DrawStyle& style);
    void DrawTextCmd(ID2D1RenderTarget* rt, const DrawCommand& cmd);
    void DrawMosaic(
        ID2D1RenderTarget* rt,
        ID2D1Bitmap* baseBitmap,
        const CaptureResult* basePixels,
        const DrawCommand& cmd);
    void DrawEraser(
        ID2D1RenderTarget* rt,
        ID2D1Bitmap* baseBitmap,
        const CaptureResult* basePixels,
        const DrawCommand& cmd);

    ID2D1Factory* m_factory = nullptr;
    IDWriteFactory* m_dwrite = nullptr;
    IWICImagingFactory* m_wic = nullptr;
    ID2D1SolidColorBrush* m_brush = nullptr;

    // A11 preview compose cache (not used for one-shot export).
    IWICBitmap* m_cacheWic = nullptr;
    ID2D1RenderTarget* m_cacheRt = nullptr;
    ID2D1Bitmap* m_cacheBaseBmp = nullptr;
    int m_cacheW = 0;
    int m_cacheH = 0;
    const void* m_cacheBaseData = nullptr;
    size_t m_cacheBaseBytes = 0;
};

} // namespace snip
