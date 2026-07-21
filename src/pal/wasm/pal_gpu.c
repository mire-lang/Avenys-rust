// WASM PAL — GPU stub
// WASM has no direct GPU access via WebGPU/WebGL from C.

#include "pal.h"
#include "../../runtime/runtime.h"

void *pal_gpu_snapshot(void) {
    void *result = rt_dict_ensure_kind(NULL, 3 /* MIRE_KIND_STR */, 3 /* MIRE_KIND_STR */);
    rt_dicts_set(result, "available", rt_managed_from_slice("false", 5));
    rt_dicts_set(result, "count", rt_managed_from_slice("0", 1));
    return result;
}
