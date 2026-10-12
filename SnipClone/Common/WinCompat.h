//==============================================================================
// SnipClone — Windows header hygiene
//
// rpcndr.h defines: #define small char  (and near/far). That breaks identifiers
// like `std::vector<...> small(...)`. Always include this after <Windows.h>.
//==============================================================================
#pragma once

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <Windows.h>

#ifdef small
#undef small
#endif
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif
#ifdef min
#undef min
#endif
#ifdef max
#undef max
#endif
