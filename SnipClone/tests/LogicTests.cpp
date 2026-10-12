//==============================================================================
// SnipClone — headless P0 regression (crop / coords / undo / compose export)
// Runs on Windows CI without interactive desktop capture.
//==============================================================================
#include "../AnnotationOverlay/AnnotationRenderer.h"
#include "../AnnotationOverlay/DrawStack.h"
#include "../Capture/CaptureResult.h"
#include "../Capture/ScreenCapture.h"
#include "../Common/CoordMap.h"
#include "../Common/DpiHelper.h"
#include "../Common/TextLayout.h"

#include <algorithm>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace
{
int g_failed = 0;
int g_passed = 0;

void Expect(bool cond, const char* name)
{
    if (cond)
    {
        ++g_passed;
        std::printf("  PASS  %s\n", name);
    }
    else
    {
        ++g_failed;
        std::printf("  FAIL  %s\n", name);
    }
}

snip::CaptureResult MakeSolid(int w, int h, RECT screenRect, std::uint8_t b, std::uint8_t g, std::uint8_t r)
{
    snip::CaptureResult img{};
    img.width = w;
    img.height = h;
    img.stride = w * 4;
    img.screenRect = screenRect;
    img.bgra.assign(static_cast<size_t>(img.stride) * static_cast<size_t>(h), 0);
    for (int y = 0; y < h; ++y)
    {
        auto* row = img.bgra.data() + static_cast<size_t>(y) * img.stride;
        for (int x = 0; x < w; ++x)
        {
            row[x * 4 + 0] = b;
            row[x * 4 + 1] = g;
            row[x * 4 + 2] = r;
            row[x * 4 + 3] = 255;
        }
    }
    // Checker so crop offsets are detectable.
    if (w > 0 && h > 0)
    {
        img.bgra[0] = 1;
        img.bgra[1] = 2;
        img.bgra[2] = 3;
        if (w > 1 && h > 1)
        {
            auto* corner = img.bgra.data() + static_cast<size_t>(h - 1) * img.stride + static_cast<size_t>(w - 1) * 4;
            corner[0] = 9;
            corner[1] = 8;
            corner[2] = 7;
        }
    }
    return img;
}

void TestCoordMap()
{
    std::printf("CoordMap\n");
    RECT origin{ -1920, 0, 0, 1080 };
    POINT screen{ -100, 50 };
    POINT image = snip::ScreenToImage(origin, screen);
    Expect(image.x == 1820 && image.y == 50, "ScreenToImage negative virtual origin");
    POINT back = snip::ImageToScreen(origin, image);
    Expect(back.x == screen.x && back.y == screen.y, "ImageToScreen round-trip");

    const RECT vs = snip::GetVirtualScreenRect();
    int x = vs.right + 500;
    int y = vs.bottom + 500;
    snip::ClampTopLeftToVirtualScreen(x, y, 100, 40);
    Expect(x + 100 <= vs.right, "clamp keeps right edge in virtual screen");
    Expect(y + 40 <= vs.bottom, "clamp keeps bottom edge in virtual screen");
    Expect(x >= vs.left && y >= vs.top, "clamp keeps origin inside virtual screen");

    x = vs.left - 200;
    y = vs.top - 200;
    snip::ClampTopLeftToVirtualScreen(x, y, 80, 60);
    Expect(x == vs.left && y == vs.top, "clamp pulls negative origin up to vs origin");
}

void TestCropToScreenRect()
{
    std::printf("CropToScreenRect\n");
    // Virtual desktop with negative origin (secondary left of primary).
    RECT vs{ -100, -50, 100, 50 }; // 200x100
    auto freeze = MakeSolid(200, 100, vs, 40, 40, 40);

    RECT want{ -50, -25, 50, 25 }; // 100x50 centered
    auto crop = freeze.CropToScreenRect(want);
    Expect(!crop.Empty(), "crop non-empty");
    Expect(crop.width == 100 && crop.height == 50, "crop size");
    Expect(crop.screenRect.left == -50 && crop.screenRect.top == -25, "crop screenRect");

    // Pixel at crop (0,0) = freeze (50,25)
    const auto* px = crop.bgra.data();
    const auto* src = freeze.bgra.data() + static_cast<size_t>(25) * freeze.stride + 50 * 4;
    Expect(px[0] == src[0] && px[1] == src[1] && px[2] == src[2], "crop pixel sample");

    // Completely outside → empty
    RECT outside{ 1000, 1000, 1100, 1100 };
    Expect(freeze.CropToScreenRect(outside).Empty(), "crop outside empty");

    // Clip to edge
    RECT overhang{ -200, -100, -80, 0 };
    auto edge = freeze.CropToScreenRect(overhang);
    Expect(!edge.Empty(), "edge crop non-empty");
    Expect(edge.screenRect.left == -100 && edge.screenRect.top == -50, "edge clip origin");
}

void TestRectsIntersect()
{
    std::printf("RectsIntersect\n");
    RECT a{ -100, 0, 0, 100 };
    RECT b{ -10, 10, 50, 50 };
    Expect(snip::RectsIntersect(a, b), "cross-monitor intersect");
    RECT c{ 10, 0, 20, 10 };
    Expect(!snip::RectsIntersect(a, c), "no intersect");
}

void TestDrawStackUndo()
{
    std::printf("DrawStack\n");
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(10, 10));
    stack.Move(D2D1::Point2F(40, 30));
    stack.Release(D2D1::Point2F(40, 30));
    Expect(stack.Shapes().size() == 1, "one shape after drag");
    Expect(stack.CanUndo(), "can undo after commit");
    stack.Undo();
    Expect(stack.Shapes().empty(), "undo clears shape");
    Expect(stack.CanRedo(), "can redo");
    stack.Redo();
    Expect(stack.Shapes().size() == 1, "redo restores shape");

    // Finish commits live stroke.
    stack.Clear();
    stack.SetTool(snip::ToolKind::Pen);
    stack.Press(D2D1::Point2F(1, 1));
    stack.Move(D2D1::Point2F(5, 5));
    Expect(stack.Live() != nullptr, "live pen while dragging");
    stack.Finish();
    Expect(stack.Live() == nullptr, "Finish clears live");
    Expect(stack.Shapes().size() == 1, "Finish commits live");

    // Move selection = one undo (hit stroke edge — unfilled rect has no interior hit).
    stack.Clear();
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(10, 10));
    stack.Move(D2D1::Point2F(30, 30));
    stack.Release(D2D1::Point2F(30, 30));
    stack.SetTool(snip::ToolKind::Select);
    stack.Press(D2D1::Point2F(10, 20));
    stack.Move(D2D1::Point2F(15, 25));
    stack.Move(D2D1::Point2F(25, 35));
    stack.Release(D2D1::Point2F(25, 35));
    Expect(stack.CanUndo(), "move creates undo");
    stack.Undo(); // undo move
    Expect(stack.CanUndo(), "draw still undoable");

    // UpdateText keeps style when provided.
    stack.Clear();
    stack.AddText(D2D1::Point2F(2, 2), L"hi");
    snip::DrawStyle st = stack.Style(snip::ToolKind::Text);
    st.bold = true;
    st.fontSize = 28.f;
    stack.UpdateText(0, L"hello", &st);
    Expect(stack.ShapeAt(0) && stack.ShapeAt(0)->style.bold && stack.ShapeAt(0)->style.fontSize == 28.f,
           "UpdateText applies style");
    Expect(stack.ShapeAt(0)->text == L"hello", "UpdateText replaces text");
}

void TestHitAccuracy()
{
    std::printf("HitTest\n");
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Arrow);
    stack.Press(D2D1::Point2F(0, 0));
    stack.Move(D2D1::Point2F(100, 0));
    stack.Release(D2D1::Point2F(100, 0));
    Expect(stack.HitTest(D2D1::Point2F(50, 0)) == 0, "arrow shaft hit");
    Expect(stack.HitTest(D2D1::Point2F(50, 40)) < 0, "arrow empty corner miss");

    stack.Clear();
    stack.SetTool(snip::ToolKind::Pen);
    stack.Press(D2D1::Point2F(0, 0));
    stack.Move(D2D1::Point2F(20, 0));
    stack.Move(D2D1::Point2F(40, 0));
    stack.Release(D2D1::Point2F(40, 0));
    Expect(stack.HitTest(D2D1::Point2F(20, 0)) == 0, "pen stroke hit");
    Expect(stack.HitTest(D2D1::Point2F(20, 30)) < 0, "pen far miss");

    stack.Clear();
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(10, 10));
    stack.Move(D2D1::Point2F(50, 40));
    stack.Release(D2D1::Point2F(50, 40));
    stack.SetTool(snip::ToolKind::Select);
    // Rect uses 8 box handles: 1=NW … 5=SE …
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(10, 10)) == 1, "resize handle NW");
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(50, 40)) == 5, "resize handle SE");
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(30, 10)) == 2, "resize handle N");
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(50, 25)) == 4, "resize handle E");

    stack.Clear();
    stack.SetTool(snip::ToolKind::Arrow);
    stack.Press(D2D1::Point2F(0, 0));
    stack.Move(D2D1::Point2F(40, 20));
    stack.Release(D2D1::Point2F(40, 20));
    stack.SetTool(snip::ToolKind::Select);
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(0, 0)) == 1, "arrow handle start");
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(40, 20)) == 2, "arrow handle end");
}

void TestBoxHandleResize()
{
    std::printf("Box handle resize\n");
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(10, 10));
    stack.Move(D2D1::Point2F(50, 40));
    stack.Release(D2D1::Point2F(50, 40));
    stack.SetTool(snip::ToolKind::Select);
    // Grab SE handle and expand.
    stack.Press(D2D1::Point2F(50, 40));
    stack.Move(D2D1::Point2F(70, 60));
    stack.Release(D2D1::Point2F(70, 60));
    const auto* sh = stack.ShapeAt(0);
    Expect(sh && sh->pts.size() >= 2, "rect still two-point");
    const float r = (std::max)(sh->pts[0].x, sh->pts[1].x);
    const float b = (std::max)(sh->pts[0].y, sh->pts[1].y);
    Expect(r >= 69.f && b >= 59.f, "SE resize expands bounds");
}

void TestRemapForCrop()
{
    std::printf("RemapForCrop\n");
    snip::DrawStack stack;
    // Use text (single anchor) to avoid two-point ordering ambiguity in asserts.
    stack.AddText(D2D1::Point2F(5.f, 5.f), L"a");
    Expect(stack.Shapes().size() == 1, "seed shape");
    Expect(stack.ShapeAt(0) && stack.ShapeAt(0)->pts.size() == 1, "text has one point");
    Expect(stack.ShapeAt(0)->pts[0].x == 5.f && stack.ShapeAt(0)->pts[0].y == 5.f, "seed at 5,5");

    // RemapForCrop applies TranslateAll(dx,dy) literally (caller negates crop delta).
    // Shrinking the crop must NOT delete shapes — they stay for restore on expand.
    stack.RemapForCrop(10.f, 10.f);
    Expect(stack.Shapes().size() == 1, "remap keeps shape");
    const auto* moved = stack.ShapeAt(0);
    Expect(moved && moved->pts[0].x == 15.f && moved->pts[0].y == 15.f, "remap translates");
    Expect(stack.CanUndo(), "remap pushes undo");

    // Optional hard cull still available for tests / future tools.
    stack.CullOutside(50.f, 50.f, 100.f, 100.f);
    Expect(stack.Shapes().empty(), "CullOutside removes fully outside shape");
}

void TestCropViewportUndo()
{
    // A01: crop + shapes share history; edge-only (dx=dy=0) still undoable with viewport.
    std::printf("Crop viewport undo\n");
    snip::DrawStack stack;
    const RECT cropA{ 100, 50, 300, 200 };
    const RECT cropB{ 100, 50, 280, 200 }; // right edge only
    stack.SetViewport(cropA);
    stack.AddText(D2D1::Point2F(20.f, 10.f), L"t");
    Expect(stack.ShapeAt(0)->pts[0].x == 20.f, "text local x before crop");

    // Simulate CommitCropRect origin shift: left moved +10 → Remap(-10,0), viewport→cropC.
    const RECT cropC{ 110, 50, 310, 200 };
    stack.SetViewport(cropA);
    stack.RemapForCrop(-10.f, 0.f);
    stack.SetViewport(cropC);
    Expect(stack.ShapeAt(0)->pts[0].x == 10.f, "text remapped after origin shift");

    RECT restored{};
    bool hasCrop = false;
    Expect(stack.Undo(&restored, &hasCrop), "undo crop remap");
    Expect(hasCrop, "undo reports crop");
    Expect(restored.left == cropA.left && restored.top == cropA.top &&
               restored.right == cropA.right && restored.bottom == cropA.bottom,
           "undo restores prior viewport");
    Expect(stack.ShapeAt(0)->pts[0].x == 20.f, "undo restores shape local coords");

    // Edge-only change: dx=dy=0 still pushes history with viewport.
    stack.SetViewport(cropA);
    stack.RemapForCrop(0.f, 0.f);
    stack.SetViewport(cropB);
    hasCrop = false;
    Expect(stack.Undo(&restored, &hasCrop), "undo edge-only crop");
    Expect(hasCrop && restored.right == cropA.right, "edge-only undo restores right");
}

void TestTextExtentConsistent()
{
    // A10: MeasureAnnotationText used by hit bounds must be finite and CJK-friendly.
    std::printf("Text extent\n");
    snip::DrawStyle style{};
    style.fontSize = 20.f;
    const auto en = snip::MeasureAnnotationText(L"Hello", style);
    const auto zh = snip::MeasureAnnotationText(L"你好世界", style);
    Expect(en.width > 10.f && en.height >= 20.f, "english extent");
    Expect(zh.width > en.width * 0.5f && zh.height >= 20.f, "chinese extent");

    snip::DrawStack stack;
    stack.AddText(D2D1::Point2F(0, 0), L"你好");
    const auto* sh = stack.ShapeAt(0);
    Expect(sh != nullptr, "text shape");
    // Hit near glyph center should succeed with shared metrics.
    Expect(stack.HitTest(D2D1::Point2F(8.f, 10.f)) == 0, "text hit via shared metrics");
}

void TestMosaicHitRect()
{
    // M1 / E02: mosaic hit uses filled AABB, not diagonal polyline.
    std::printf("Mosaic hit rect\n");
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Mosaic);
    stack.Press(D2D1::Point2F(10, 10));
    stack.Move(D2D1::Point2F(60, 40));
    stack.Release(D2D1::Point2F(60, 40));
    stack.SetTool(snip::ToolKind::Select);
    Expect(stack.HitTest(D2D1::Point2F(35, 25)) == 0, "mosaic center hit");
    Expect(stack.HitTest(D2D1::Point2F(55, 12)) == 0, "mosaic near edge off-diagonal hit");
    Expect(stack.HitTest(D2D1::Point2F(80, 80)) < 0, "mosaic far miss");
}

void TestStyleEditsSelected()
{
    // M1 / E01: color/width mutate selected shape and push undo.
    std::printf("Style edits selected\n");
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(0, 0));
    stack.Move(D2D1::Point2F(40, 30));
    stack.Release(D2D1::Point2F(40, 30));
    Expect(stack.SelectedIndex() == 0, "auto-select after create");
    stack.SetTool(snip::ToolKind::Select);
    Expect(stack.SelectedIndex() == 0, "selection kept in Select");

    const float w0 = stack.ShapeAt(0)->style.width;
    stack.AdjustWidth(+2);
    Expect(stack.ShapeAt(0)->style.width == w0 + 2.f, "width edits selected");
    Expect(stack.CanUndo(), "style change undoable");
    stack.SetColorRgb(0.f, 0.f, 1.f);
    Expect(stack.ShapeAt(0)->style.color.b > 0.9f, "color edits selected");
    Expect(stack.Undo(), "undo color");
    Expect(stack.ShapeAt(0)->style.width == w0 + 2.f, "undo restores prior style frame");
    Expect(stack.Undo(), "undo width");
    Expect(stack.ShapeAt(0)->style.width == w0, "width restored");

    Expect(stack.DuplicateSelected(), "duplicate selected");
    Expect(stack.Shapes().size() == 2, "duplicate adds shape");
    Expect(stack.SelectedIndex() == 1, "duplicate selects copy");
}

void TestComposeFailsEmpty()
{
    // A02: failure / empty base must not return a non-empty silent base copy.
    std::printf("Compose fail empty\n");
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    snip::AnnotationRenderer renderer;
    snip::DrawStack stack;
    snip::CaptureResult empty;
    bool ok = true;
    auto out = renderer.Compose(empty, stack, false, false, &ok);
    Expect(out.Empty(), "empty base → empty compose");
    Expect(!ok, "empty base → ok=false");
    renderer.Shutdown();
    CoUninitialize();
}

void TestComposeReuseSurface()
{
    // A11: reuseSurface=true must match one-shot compose pixels and stay non-empty.
    std::printf("Compose reuse surface\n");
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    RECT sr{ 0, 0, 32, 24 };
    auto base = MakeSolid(32, 24, sr, 10, 20, 30);
    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Pen);
    stack.Press(D2D1::Point2F(2, 2));
    stack.Move(D2D1::Point2F(20, 16));
    stack.Release(D2D1::Point2F(20, 16));

    snip::AnnotationRenderer renderer;
    if (!renderer.Initialize())
    {
        Expect(false, "reuse renderer init");
        CoUninitialize();
        return;
    }
    bool ok1 = false;
    bool ok2 = false;
    auto a = renderer.Compose(base, stack, false, false, &ok1, false);
    auto b = renderer.Compose(base, stack, false, false, &ok2, true);
    Expect(ok1 && ok2 && !a.Empty() && !b.Empty(), "reuse + one-shot both succeed");
    Expect(a.bgra == b.bgra, "reuse surface matches one-shot pixels");
    auto c = renderer.Compose(base, stack, false, false, nullptr, true);
    Expect(!c.Empty() && c.bgra == b.bgra, "second reuse frame stable");
    renderer.ReleaseComposeCache();
    renderer.Shutdown();
    CoUninitialize();
}

void TestComposeExportOmitsSelectionChrome()
{
    std::printf("Compose export chrome\n");
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

    RECT sr{ 0, 0, 64, 48 };
    auto base = MakeSolid(64, 48, sr, 20, 30, 40);

    snip::DrawStack stack;
    stack.SetTool(snip::ToolKind::Rect);
    stack.Press(D2D1::Point2F(8, 8));
    stack.Move(D2D1::Point2F(40, 28));
    stack.Release(D2D1::Point2F(40, 28));
    Expect(stack.SelectedIndex() >= 0, "shape selected after commit");

    snip::AnnotationRenderer renderer;
    if (!renderer.Initialize())
    {
        Expect(false, "renderer Initialize");
        CoUninitialize();
        return;
    }

    auto withChrome = renderer.Compose(base, stack, true, false);
    auto exportImg = renderer.Compose(base, stack, false, false);
    Expect(!withChrome.Empty() && !exportImg.Empty(), "compose outputs non-empty");
    Expect(withChrome.bgra.size() == exportImg.bgra.size(), "compose size match");

    // Selection chrome draws blue outline; export must differ when selected.
    bool differ = withChrome.bgra != exportImg.bgra;
    Expect(differ, "selection chrome changes pixels vs export");

    // Cleared selection + chrome-on should match export (no outline).
    stack.ClearSelection();
    auto cleared = renderer.Compose(base, stack, true, false);
    Expect(cleared.bgra == exportImg.bgra, "export matches deselected compose");

    renderer.Shutdown();
    CoUninitialize();
}
} // namespace

int wmain()
{
    std::printf("SnipClone LogicTests\n");
    TestCoordMap();
    TestCropToScreenRect();
    TestRectsIntersect();
    TestDrawStackUndo();
    TestHitAccuracy();
    TestBoxHandleResize();
    TestMosaicHitRect();
    TestStyleEditsSelected();
    TestRemapForCrop();
    TestCropViewportUndo();
    TestTextExtentConsistent();
    TestComposeFailsEmpty();
    TestComposeReuseSurface();
    TestComposeExportOmitsSelectionChrome();

    std::printf("\n%d passed, %d failed\n", g_passed, g_failed);
    return g_failed == 0 ? 0 : 1;
}
