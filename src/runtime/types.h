#ifndef MIRE_TYPES_H
#define MIRE_TYPES_H

#include <stdint.h>

// ═══════════════════════════════════════════════════════════════════════
//  Maybe[T] — pointer-based: NULL=None, non-NULL=ptr to heap value
//  NOTE: The compiler codegens Maybe as ptr. Inline structs require
//  compiler codegen changes. For now, C functions manage heap values.
// ═══════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════
//  Result[T E] — pointer to {tag: i64, payload: i64}
//  tag=1 → Ok, tag=0 → Err
// ═══════════════════════════════════════════════════════════════════════

typedef struct {
    int64_t tag;     // 1=Ok, 0=Err
    int64_t payload; // value or error
} MireResult;

// ═══════════════════════════════════════════════════════════════════════
//  Arr[T N] — runtime receives (pointer, count)
// ═══════════════════════════════════════════════════════════════════════

typedef struct {
    void *data;
    int64_t count;
} MireArr;

#endif // MIRE_TYPES_H
