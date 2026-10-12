//==============================================================================
// SnipClone — capture output (BGRA top-down)
//==============================================================================
#pragma once

#include <Windows.h>

#include <cstdint>
#include <vector>

namespace snip
{

struct CaptureResult
{
    std::vector<std::uint8_t> bgra; // row-major, top-down, 32bpp BGRA
    int width = 0;
    int height = 0;
    int stride = 0; // bytes per row (typically width * 4, 4-byte aligned)
    RECT screenRect{};

    bool Empty() const { return bgra.empty() || width <= 0 || height <= 0; }

    SIZE Size() const { return { width, height }; }
};

} // namespace snip
