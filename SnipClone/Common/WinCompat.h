//==============================================================================
// SnipClone — Windows header hygiene
//
// rpcndr.h: `#define small char` breaks names like `vector<...> small(...)`.
// Only undef `small` / `min` / `max`.
//
// NEVER undef `far` / `near`: windef maps FAR→far. Undefining far turns
// `EXTERN_C const GUID FAR iid` into `... GUID far iid` and cascades into
// thousands of IID_/GUID_ parse errors in d3d/dxgi/wincodec headers.
//==============================================================================
#pragma once

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <Windows.h>

#ifdef small
#undef small
#endif
#ifdef min
#undef min
#endif
#ifdef max
#undef max
#endif
