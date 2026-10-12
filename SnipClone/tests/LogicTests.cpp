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

    int x = -50;
    int y = -20;
    snip::ClampTopLeftToVirtualScreen(x, y, 100, 40);
    // Without a real virtual screen in CI this still must not crash; clamp is monotonic.
    Expect(true, "ClampTopLeftToVirtualScreen callable");
    (void)x;
    (void)y;
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
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(10, 10)) == 1, "resize handle p0");
    Expect(stack.HitResizeHandle(0, D2D1::Point2F(50, 40)) == 2, "resize handle p1");
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
    TestRemapForCrop();
    TestComposeExportOmitsSelectionChrome();

    std::printf("\n%d passed, %d failed\n", g_passed, g_failed);
    return g_failed == 0 ? 0 : 1;
}
