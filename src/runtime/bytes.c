// bytes.c — Pure byte/buffer access helpers (PAL-free).
// Extracted from helpers.c so the minimal runtime tier can pull in the
// raw memory readers/writers (used by math.c and FFI bindings) WITHOUT
// dragging helpers.c's PAL-dependent file/process/crypto functions.

#include <stdint.h>
#include <stdlib.h>
#include "runtime.h"

// ── Raw buffer allocation ─────────────────────────────────────────
// Unmanaged malloc/free for passing opaque buffers (SDL_Event, SDL_Rect,
// pixel data) across FFI. Caller is responsible for pairing these; the
// runtime never touches the memory (not managed, not GC'd).
void *rt_alloc_raw(int64_t size) {
    if (size <= 0) return NULL;
    return malloc((size_t)size);
}
void rt_free_raw(void *ptr) {
    if (ptr) free(ptr);
}

// ── Raw memory readers (little-endian) ────────────────────────────
// Little-endian typed reads from arbitrary buffers (used by FFI bindings
// to decode C structs such as SDL_Event without needing struct support).
int64_t rt_read_u8(const void *ptr) {
    return (int64_t)(unsigned char)((const unsigned char *)ptr)[0];
}
int64_t rt_read_u16(const void *ptr) {
    const unsigned char *p = (const unsigned char *)ptr;
    return (int64_t)(p[0] | ((unsigned int)p[1] << 8));
}
int64_t rt_read_u32(const void *ptr) {
    const unsigned char *p = (const unsigned char *)ptr;
    return (int64_t)((unsigned int)p[0] | ((unsigned int)p[1] << 8) |
                     ((unsigned int)p[2] << 16) | ((unsigned int)p[3] << 24));
}
// Blend `color` (RGBA packed) into the u32 at `ptr` with the given alpha
// (0..255). Used for anti-aliased font edges and translucent shadows.
void rt_blend_u32(void *ptr, int64_t color, int64_t alpha) {
    if (ptr == NULL) return;
    if (alpha <= 0) return;
    if (alpha >= 255) {
        rt_write_u32(ptr, color);
        return;
    }
    uint32_t dst = (uint32_t)rt_read_u32(ptr);
    uint32_t src = (uint32_t)color;
    int a = (int)alpha;
    int ia = 255 - a;
    uint8_t r = (uint8_t)(((src & 0xFF) * a + (dst & 0xFF) * ia) / 255);
    uint8_t g = (uint8_t)((((src >> 8) & 0xFF) * a + ((dst >> 8) & 0xFF) * ia) / 255);
    uint8_t b = (uint8_t)((((src >> 16) & 0xFF) * a + ((dst >> 16) & 0xFF) * ia) / 255);
    uint8_t al = (uint8_t)((((src >> 24) & 0xFF) * a + ((dst >> 24) & 0xFF) * ia) / 255);
    uint32_t out = (uint32_t)r | ((uint32_t)g << 8) | ((uint32_t)b << 16) | ((uint32_t)al << 24);
    rt_write_u32(ptr, out);
}
int64_t rt_read_i32(const void *ptr) {
    const unsigned char *p = (const unsigned char *)ptr;
    uint32_t v = (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
                 ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
    return (int64_t)(int32_t)v;
}
int64_t rt_read_u64(const void *ptr) {
    uint64_t v = 0;
    const unsigned char *p = (const unsigned char *)ptr;
    for (int i = 0; i < 8; i++) v |= (uint64_t)p[i] << (8 * i);
    return (int64_t)v;
}
int64_t rt_read_ptr(const void *ptr) {
    return rt_read_u64(ptr);
}
double rt_read_f64(const void *ptr) {
    union { uint64_t u; double f; } conv;
    conv.u = 0;
    const unsigned char *p = (const unsigned char *)ptr;
    for (int i = 0; i < 8; i++) conv.u |= (uint64_t)p[i] << (8 * i);
    return conv.f;
}
double rt_read_f32(const void *ptr) {
    union { uint32_t u; float f; } conv;
    conv.u = 0;
    const unsigned char *p = (const unsigned char *)ptr;
    for (int i = 0; i < 4; i++) conv.u |= (uint32_t)p[i] << (8 * i);
    return (double)conv.f;
}

// ── Raw memory writers ────────────────────────────────────────────
void rt_write_u8(void *ptr, int64_t value) {
    ((unsigned char *)ptr)[0] = (unsigned char)value;
}
void rt_write_u16(void *ptr, int64_t value) {
    unsigned char *p = (unsigned char *)ptr;
    p[0] = (unsigned char)value;
    p[1] = (unsigned char)(value >> 8);
}
void rt_write_u32(void *ptr, int64_t value) {
    unsigned char *p = (unsigned char *)ptr;
    p[0] = (unsigned char)value;
    p[1] = (unsigned char)(value >> 8);
    p[2] = (unsigned char)(value >> 16);
    p[3] = (unsigned char)(value >> 24);
}
void rt_write_i32(void *ptr, int64_t value) {
    rt_write_u32(ptr, (int64_t)(int32_t)value);
}
void rt_write_f32(void *ptr, double value) {
    union { uint32_t u; float f; } conv;
    conv.f = (float)value;
    rt_write_u32(ptr, (int64_t)conv.u);
}
void rt_write_f64(void *ptr, double value) {
    union { uint64_t u; double f; } conv;
    conv.f = value;
    rt_write_u64(ptr, (int64_t)conv.u);
}
void rt_write_u64(void *ptr, int64_t value) {
    unsigned char *p = (unsigned char *)ptr;
    uint64_t v = (uint64_t)value;
    for (int i = 0; i < 8; i++) p[i] = (unsigned char)(v >> (8 * i));
}

// ── C-string → managed string ────────────────────────────────────
// Reads a null-terminated C string at `ptr` and returns a managed copy.
// Used by FFI bindings to decode `const char *` fields inside C structs
// (e.g. SDL_TextInputEvent.text in SDL3).
char *rt_read_cstr(const void *ptr) {
    return rt_string_copy((const char *)ptr);
}