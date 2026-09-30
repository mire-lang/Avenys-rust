// Weak defaults for optional browser-oriented WASM services.
// Native embedders may replace these symbols with strong implementations;
// WASI uses the corresponding adapter when the target selects pal/wasi.
#include "pal.h"

#if defined(__GNUC__) || defined(__clang__)
#define PAL_OPTIONAL_WEAK __attribute__((weak))
#else
#define PAL_OPTIONAL_WEAK
#endif

PAL_OPTIONAL_WEAK int64_t wasm_fetch_get(const char *url) {
    (void)url;
    return -1;
}
PAL_OPTIONAL_WEAK int64_t wasm_fetch_status(int64_t request) {
    (void)request;
    return -1;
}
PAL_OPTIONAL_WEAK int64_t wasm_fetch_read(int64_t request, char *buffer, int64_t len) {
    (void)request; (void)buffer; (void)len;
    return -1;
}
PAL_OPTIONAL_WEAK void wasm_fetch_close(int64_t request) { (void)request; }

PAL_OPTIONAL_WEAK const char *wasm_storage_get(const char *key) {
    (void)key;
    return "";
}
PAL_OPTIONAL_WEAK bool wasm_storage_set(const char *key, const char *value) {
    (void)key; (void)value;
    return false;
}
PAL_OPTIONAL_WEAK bool wasm_storage_remove(const char *key) {
    (void)key;
    return false;
}

PAL_OPTIONAL_WEAK int64_t wasm_canvas_width(void) { return 0; }
PAL_OPTIONAL_WEAK int64_t wasm_canvas_height(void) { return 0; }
PAL_OPTIONAL_WEAK void wasm_canvas_fill_rect(int64_t x, int64_t y, int64_t width,
                                              int64_t height, int64_t color) {
    (void)x; (void)y; (void)width; (void)height; (void)color;
}
