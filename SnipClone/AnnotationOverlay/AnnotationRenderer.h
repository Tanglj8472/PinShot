//==============================================================================
// SnipClone — D2D annotation renderer（底层绘制参考 ZoomIt 抗锯齿矢量思路）
//==============================================================================
#pragma once

#include "../Capture/CaptureResult.h"
#include "DrawCommand.h"
#include "DrawStack.h"

#include <d2d1.h>
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

    // Draw base + shapes into an existing DC render target (already BeginDraw).
    void DrawScene(
        ID2D1RenderTarget* rt,
        ID2D1Bitmap* baseBitmap,
        const DrawStack& stack);

    // Merge base BGRA + shapes → new CaptureResult (PinShot composed()).
    CaptureResult Compose(const CaptureResult& base, const DrawStack& stack);

    ID2D1Bitmap* CreateBitmapFromCapture(ID2D1RenderTarget* rt, const CaptureResult& image);

    ID2D1Factory* Factory() const { return m_factory; }

private:
    void EnsureBrushes(ID2D1RenderTarget* rt);
    void DrawCommandOn(ID2D1RenderTarget* rt, const DrawCommand& cmd);
    void DrawArrowHead(ID2D1RenderTarget* rt, D2D1_POINT_2F from, D2D1_POINT_2F to, const DrawStyle& style);

    ID2D1Factory* m_factory = nullptr;
    IWICImagingFactory* m_wic = nullptr;
    ID2D1SolidColorBrush* m_brush = nullptr;
};

} // namespace snip
