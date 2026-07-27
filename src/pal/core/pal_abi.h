#ifndef MIRE_PAL_ABI_H
#define MIRE_PAL_ABI_H

#include "../pal.h"

// pal_abi.h — PAL Core internal types.
// Includes pal.h for all public types (handles, errors, flags, etc.)
// Only adds Core-internal types not exposed in the ABI.

#define PAL_ABI_VERSION 4

// pal_handle_t is already defined in pal.h.

// Handle validation (called by PAL Core)
bool pal_handle_is_valid(pal_handle_t h);

#endif
