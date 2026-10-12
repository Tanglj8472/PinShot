//==============================================================================
// SnipClone — vector toolbar icons (PinShot icons.py geometry, D2D)
//==============================================================================
#include "ToolIcons.h"

#include "../Common/WinCompat.h"

#include <dwrite.h>

#include <cmath>

#pragma comment(lib, "dwrite.lib")

namespace snip
{
namespace
{
struct Pt
{
    float x, y;
};

D2D1_RECT_F Inset(const D2D1_RECT_F& cell, float pad)
{
    return D2D1::RectF(cell.left + pad, cell.top + pad, cell.right - pad, cell.bottom - pad);
}

void Map(const D2D1_RECT_F& box, float u, float v, float* x, float* y)
{
    // u,v in 0..20 icon space
    const float w = box.right - box.left;
    const float h = box.bottom - box.top;
    *x = box.left + u / 20.f * w;
    *y = box.top + v / 20.f * h;
}

void Line(ID2D1RenderTarget* rt, ID2D1SolidColorBrush* brush, const D2D1_RECT_F& box, float stroke,
          float x0, float y0, float x1, float y1)
{
    float ax, ay, bx, by;
    Map(box, x0, y0, &ax, &ay);
    Map(box, x1, y1, &bx, &by);
    rt->DrawLine(D2D1::Point2F(ax, ay), D2D1::Point2F(bx, by), brush, stroke);
}

void ArrowHead(ID2D1RenderTarget* rt, ID2D1SolidColorBrush* brush, const D2D1_RECT_F& box, float stroke,
               float x0, float y0, float x1, float y1)
{
    float ax, ay, bx, by;
    Map(box, x0, y0, &ax, &ay);
    Map(box, x1, y1, &bx, &by);
    rt->DrawLine(D2D1::Point2F(ax, ay), D2D1::Point2F(bx, by), brush, stroke);
    const float ang = std::atan2(by - ay, bx - ax);
    const float head = 5.f * (box.right - box.left) / 20.f;
    const D2D1_POINT_2F p0 = D2D1::Point2F(bx, by);
    const D2D1_POINT_2F p1 = D2D1::Point2F(
        bx - head * std::cos(ang - 0.5f),
        by - head * std::sin(ang - 0.5f));
    const D2D1_POINT_2F p2 = D2D1::Point2F(
        bx - head * std::cos(ang + 0.5f),
        by - head * std::sin(ang + 0.5f));
    ID2D1PathGeometry* geo = nullptr;
    ID2D1Factory* factory = nullptr;
    rt->GetFactory(&factory);
    if (!factory)
    {
        return;
    }
    factory->CreatePathGeometry(&geo);
    factory->Release();
    if (!geo)
    {
        return;
    }
    ID2D1GeometrySink* sink = nullptr;
    geo->Open(&sink);
    if (sink)
    {
        sink->BeginFigure(p0, D2D1_FIGURE_BEGIN_FILLED);
        sink->AddLine(p1);
        sink->AddLine(p2);
        sink->EndFigure(D2D1_FIGURE_END_CLOSED);
        sink->Close();
        sink->Release();
        rt->FillGeometry(geo, brush);
    }
    geo->Release();
}
} // namespace

void DrawToolbarIcon(
    ID2D1RenderTarget* rt,
    ID2D1SolidColorBrush* brush,
    const D2D1_RECT_F& cell,
    ToolbarAction action,
    ToolKind tool)
{
    if (!rt || !brush)
    {
        return;
    }
    const D2D1_RECT_F box = Inset(cell, 6.f);
    const float stroke = 1.6f;
    const float w = box.right - box.left;
    const float h = box.bottom - box.top;

    if (action == ToolbarAction::SetTool)
    {
        switch (tool)
        {
        case ToolKind::Select:
            rt->DrawRectangle(
                D2D1::RectF(box.left + w * 0.15f, box.top + h * 0.15f, box.left + w * 0.65f, box.top + h * 0.65f),
                brush,
                stroke);
            Line(rt, brush, box, stroke, 14, 14, 17, 17);
            return;
        case ToolKind::Pen:
        {
            ID2D1PathGeometry* geo = nullptr;
            ID2D1Factory* factory = nullptr;
            rt->GetFactory(&factory);
            if (factory)
            {
                factory->CreatePathGeometry(&geo);
                factory->Release();
            }
            if (geo)
            {
                ID2D1GeometrySink* sink = nullptr;
                geo->Open(&sink);
                if (sink)
                {
                    float x, y;
                    Map(box, 3, 14, &x, &y);
                    sink->BeginFigure(D2D1::Point2F(x, y), D2D1_FIGURE_BEGIN_HOLLOW);
                    float c1x, c1y, c2x, c2y, ex, ey;
                    Map(box, 6, 4, &c1x, &c1y);
                    Map(box, 10, 18, &c2x, &c2y);
                    Map(box, 17, 5, &ex, &ey);
                    sink->AddBezier(D2D1::BezierSegment(
                        D2D1::Point2F(c1x, c1y),
                        D2D1::Point2F(c2x, c2y),
                        D2D1::Point2F(ex, ey)));
                    sink->EndFigure(D2D1_FIGURE_END_OPEN);
                    sink->Close();
                    sink->Release();
                    rt->DrawGeometry(geo, brush, stroke);
                }
                geo->Release();
            }
            return;
        }
        case ToolKind::Rect:
            rt->DrawRectangle(
                D2D1::RectF(box.left + w * 0.15f, box.top + h * 0.25f, box.right - w * 0.15f, box.bottom - h * 0.25f),
                brush,
                stroke);
            return;
        case ToolKind::Ellipse:
            rt->DrawEllipse(
                D2D1::Ellipse(
                    D2D1::Point2F((box.left + box.right) * 0.5f, (box.top + box.bottom) * 0.5f),
                    w * 0.35f,
                    h * 0.28f),
                brush,
                stroke);
            return;
        case ToolKind::Arrow:
            ArrowHead(rt, brush, box, stroke, 4, 16, 16, 4);
            return;
        case ToolKind::Text:
        {
            // Bold "T"
            IDWriteFactory* dw = nullptr;
            if (SUCCEEDED(DWriteCreateFactory(
                    DWRITE_FACTORY_TYPE_SHARED,
                    __uuidof(IDWriteFactory),
                    reinterpret_cast<IUnknown**>(&dw))) &&
                dw)
            {
                IDWriteTextFormat* fmt = nullptr;
                dw->CreateTextFormat(
                    L"Segoe UI",
                    nullptr,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    h * 0.75f,
                    L"en-us",
                    &fmt);
                if (fmt)
                {
                    fmt->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                    fmt->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                    rt->DrawText(L"T", 1, fmt, box, brush);
                    fmt->Release();
                }
                dw->Release();
            }
            return;
        }
        case ToolKind::Mosaic:
            for (int i = 0; i < 3; ++i)
            {
                for (int j = 0; j < 3; ++j)
                {
                    if (((i + j) & 1) == 0)
                    {
                        const float x0 = box.left + w * (0.2f + i * 0.2f);
                        const float y0 = box.top + h * (0.2f + j * 0.2f);
                        rt->FillRectangle(
                            D2D1::RectF(x0, y0, x0 + w * 0.2f, y0 + h * 0.2f),
                            brush);
                    }
                }
            }
            rt->DrawRectangle(
                D2D1::RectF(box.left + w * 0.2f, box.top + h * 0.2f, box.right - w * 0.2f, box.bottom - h * 0.2f),
                brush,
                1.f);
            return;
        case ToolKind::Eraser:
            Line(rt, brush, box, stroke, 4, 13, 11, 5);
            Line(rt, brush, box, stroke, 11, 5, 16, 10);
            Line(rt, brush, box, stroke, 16, 10, 10, 16);
            Line(rt, brush, box, stroke, 10, 16, 6, 16);
            Line(rt, brush, box, stroke, 6, 16, 4, 13);
            Line(rt, brush, box, stroke, 7.5f, 9, 13, 14);
            Line(rt, brush, box, stroke, 10, 16, 17, 16);
            return;
        default:
            break;
        }
    }

    switch (action)
    {
    case ToolbarAction::Undo:
        // Arc-ish chevron left
        Line(rt, brush, box, stroke, 12, 5, 6, 10);
        Line(rt, brush, box, stroke, 6, 10, 12, 15);
        Line(rt, brush, box, stroke, 6, 10, 16, 10);
        return;
    case ToolbarAction::Redo:
        Line(rt, brush, box, stroke, 8, 5, 14, 10);
        Line(rt, brush, box, stroke, 14, 10, 8, 15);
        Line(rt, brush, box, stroke, 4, 10, 14, 10);
        return;
    case ToolbarAction::Close:
        brush->SetColor(D2D1::ColorF(0.82f, 0.2f, 0.2f, 1.f));
        Line(rt, brush, box, 1.8f, 5, 5, 15, 15);
        Line(rt, brush, box, 1.8f, 15, 5, 5, 15);
        return;
    case ToolbarAction::Pin:
        Line(rt, brush, box, stroke, 10, 12, 10, 18);
        rt->DrawRectangle(
            D2D1::RectF(box.left + w * 0.3f, box.top + h * 0.15f, box.right - w * 0.3f, box.top + h * 0.5f),
            brush,
            stroke);
        Line(rt, brush, box, stroke, 4, 11, 16, 11);
        return;
    case ToolbarAction::Save:
        Line(rt, brush, box, stroke, 10, 3, 10, 12);
        ArrowHead(rt, brush, box, stroke, 10, 3, 10, 13);
        Line(rt, brush, box, stroke, 4, 12, 4, 16);
        Line(rt, brush, box, stroke, 4, 16, 16, 16);
        Line(rt, brush, box, stroke, 16, 16, 16, 12);
        return;
    case ToolbarAction::Copy:
        rt->DrawRectangle(
            D2D1::RectF(box.left + w * 0.35f, box.top + h * 0.15f, box.right - w * 0.15f, box.bottom - h * 0.3f),
            brush,
            stroke);
        rt->DrawRectangle(
            D2D1::RectF(box.left + w * 0.15f, box.top + h * 0.3f, box.right - w * 0.35f, box.bottom - h * 0.15f),
            brush,
            stroke);
        return;
    case ToolbarAction::WidthDec:
        Line(rt, brush, box, 2.f, 5, 10, 15, 10);
        return;
    case ToolbarAction::WidthInc:
        Line(rt, brush, box, 2.f, 5, 10, 15, 10);
        Line(rt, brush, box, 2.f, 10, 5, 10, 15);
        return;
    case ToolbarAction::ToggleFill:
        rt->DrawRectangle(
            D2D1::RectF(box.left + w * 0.2f, box.top + h * 0.2f, box.right - w * 0.2f, box.bottom - h * 0.2f),
            brush,
            stroke);
        rt->FillRectangle(
            D2D1::RectF(box.left + w * 0.35f, box.top + h * 0.35f, box.right - w * 0.35f, box.bottom - h * 0.35f),
            brush);
        return;
    case ToolbarAction::Bold:
    case ToolbarAction::Italic:
    case ToolbarAction::Outline:
        // Drawn as text by caller when needed
        return;
    default:
        break;
    }
}

} // namespace snip
