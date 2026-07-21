// WASM PAL — GPU (platform-limited)
// WASM runs inside a sandboxed environment with no direct GPU enumeration.
// WebGPU/WebGL access requires JavaScript interop not available from C.
// This reports the honest truth: no GPU info is available.

#include "pal.h"
#include "../../runtime/runtime.h"

void *pal_gpu_snapshot(void) {
    void *result = rt_dict_ensure_kind(NULL, 3 /* MIRE_KIND_STR */, 3 /* MIRE_KIND_STR */);
    rt_dicts_set(result, "available", rt_managed_from_slice("false", 5));
    rt_dicts_set(result, "count", rt_managed_from_slice("0", 1));
    rt_dicts_set(result, "reason", rt_managed_from_slice("wasm_sandbox", 12));
    return result;
}
