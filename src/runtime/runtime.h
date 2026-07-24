#ifndef MIRE_RUNTIME_H
#define MIRE_RUNTIME_H

// ═══════════════════════════════════════════════════════════════════════
//  Mire Runtime — public C API
//
//  This header declares all runtime functions available to compiled Mire
//  programs. Functions are grouped by module:
//
//    1. Managed memory   — heap-allocated strings with reference tracking
//    2. Strings          — string manipulation, case, search, split/join
//    3. Vecs             — dynamic arrays (vec[T])
//    4. Maps             — hash maps (map[K V])
//    5. Maybe[T]         — optional values (Some/None)
//    6. Result[T E]      — error handling (Ok/Err) + ? operator
//    7. Arr[T N]         — fixed-size arrays
//    8. Math             — trigonometry, statistics, range generation
//    9. Safety           — bounds checks, checked arithmetic, panics
//   10. Crypto           — SHA-256, HMAC, base64, hex encoding
//   11. I/O              — print, read line, arguments
//   12. PAL (platform)   — filesystem, networking, process, threads
//
//  Backward-compat aliases:
//    rt_lists_* → rt_vecs_*   (lists.c renamed to vecs.c)
//    rt_dicts_* → rt_maps_*   (dicts.c renamed to maps.c)
//
//  Internal helpers (not part of the public ABI):
//    maps_internal.h — hash map internals
//    types.h         — MireResult struct, MIRE_KIND_* constants
// ═══════════════════════════════════════════════════════════════════════

#include <stddef.h>
#include <stdint.h>

// ── Cross-platform export macros ─────────────────────────────────────
// WASM: mark functions for JavaScript interop via wasm-ld
// Native: no-op (default visibility)
#if defined(__wasm__)
  #define MIRE_EXPORT __attribute__((visibility("default")))
  #define MIRE_WASM_EXPORT(name) __attribute__((export_name(name)))
#else
  #define MIRE_EXPORT
  #define MIRE_WASM_EXPORT(name)
#endif

// ═══════════════════════════════════════════════════════════════════════
//  1. Managed memory
//
//  Mire uses a managed allocator for all heap strings. Every string
//  returned by the runtime is managed unless noted otherwise. Managed
//  strings have a hidden header with length/capacity metadata.
// ═══════════════════════════════════════════════════════════════════════

#define MIRE_STR_MANAGED     1  // allocated via managed allocator
#define MIRE_STR_UTF8_KNOWN  2  // utf8_cp field is valid

typedef struct {
    size_t len;       // byte length (excluding NUL)
    size_t cap;       // allocated capacity (bytes)
    uint32_t flags;   // MIRE_STR_* flags
    uint32_t utf8_cp; // cached codepoint count (valid when MIRE_STR_UTF8_KNOWN set)
    char data[];      // flexible array member — UTF-8 bytes + NUL
} MireManagedString;

char *rt_managed_alloc(size_t len);
char *rt_managed_from_slice(const char *src, size_t len);
char *rt_managed_from_cstr(const char *src);
char *rt_managed_ensure_managed(char *ptr);
char *rt_managed_printf_i64(const char *fmt, long long value);
char *rt_managed_printf_f64(const char *fmt, double value);
void  rt_managed_free(char *value);
void  rt_managed_cleanup_all(void);
int   rt_managed_is_managed(const char *value);
size_t rt_managed_len(const char *value);
int   rt_managed_contains(const char *data_ptr);
void  rt_managed_register(char *data_ptr);
void  rt_managed_unregister(char *data_ptr);

char *rt_strdup_raw(const char *src);
char *rt_strdup_raw_n(const char *src, size_t len);
char *rt_alloc_printf_raw_i64(const char *fmt, long long value);
size_t rt_string_growth_cap(size_t min_cap);
MireManagedString *rt_string_header(const char *data);

// ═══════════════════════════════════════════════════════════════════════
//  2. String operations
// ═══════════════════════════════════════════════════════════════════════

char *rt_string_copy(const char *value);
char *rt_string_concat(const char *left, const char *right);
char *rt_strings_repeat(const char *input, int64_t count);
char *rt_string_append_owned(char *value, const char *suffix);
int64_t rt_strings_len(const char *s);
int64_t rt_string_to_i64(const char *value);

// UTF-8 aware operations (work on codepoints, not bytes)
int64_t rt_strings_len_utf8(const char *s);
char   *rt_strings_substr_utf8(const char *input, int64_t start_cp, int64_t count_cp);
int64_t rt_strings_index_of_utf8(const char *s, const char *sub);

// Case conversion
char *rt_string_to_upper(const char *value);
char *rt_string_to_lower(const char *value);
char  rt_unicode_to_lower(unsigned char c);
char  rt_unicode_to_upper(unsigned char c);

// Number-to-string conversions
char *rt_i64_to_string(int64_t value);
char *rt_bool_to_string(int64_t value);
char *rt_f64_to_string(double value);
char *rt_f32_to_string(float value);
char *rt_i128_to_string(__int128 value);
char *rt_u128_to_string(unsigned __int128 value);

// Search, replace, trim, split, join
int64_t rt_strings_contains(const char *input, const char *needle);
char   *rt_strings_replace(const char *input, const char *from, const char *to);
char   *rt_strings_replace_first(const char *input, const char *from, const char *to);
int64_t rt_strings_starts_with(const char *str, const char *prefix);
int64_t rt_strings_ends_with(const char *str, const char *suffix);
char   *rt_strings_substr(const char *input, int64_t start, int64_t length);
char   *rt_strings_pad_left(const char *input, int64_t width, const char *pad);
char   *rt_strings_pad_right(const char *input, int64_t width, const char *pad);
char   *rt_strings_trim(const char *input);
char   *rt_strings_split_list(const char *input, const char *delimiter);
char   *rt_strings_join(char **parts, int64_t count, const char *delimiter);
int64_t rt_strings_index_of(const char *s, const char *sub);

// String module convenience aliases
int64_t rt_strings_to_upper(const char *s);
int64_t rt_strings_to_lower(const char *s);
int64_t rt_strings_strip(const char *s);
void   *rt_strings_split(const char *s, const char *sep);
char   *rt_strings_join_list(void *parts, const char *sep);

// ═══════════════════════════════════════════════════════════════════════
//  3. Vecs — dynamic arrays (vec[T])
//
//  Internal layout: [capacity: i64, length: i64, elements...]
//  All functions return a (possibly reallocated) pointer.
// ═══════════════════════════════════════════════════════════════════════

void *rt_list_create(int64_t initial_cap, int64_t elem_size);
int64_t rt_list_len(void *list_ptr);
void *rt_list_push_i64(void *list_ptr, int64_t value);
void *rt_list_push_ptr(void *list_ptr, void *value);
void *rt_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size);
int64_t rt_list_pop_i64(void *list_ptr);
void *rt_list_concat(void *left_ptr, void *right_ptr);
void *rt_list_slice(void *list_ptr, int64_t start, int64_t end);
void *rt_list_remove(void *list_ptr, int64_t index);
void *rt_list_clear(void *list_ptr);
int64_t rt_list_get_i64(void *list_ptr, int64_t index);
void   rt_list_set_i64(void *list_ptr, int64_t index, int64_t value);
void *rt_list_get_ptr(void *list_ptr, int64_t index);
void  rt_list_free(void *list);
void *rt_lists_flatten(void *list);
void *rt_lists_sort(void *list);
int64_t rt_lists_contains_i64(void *list, int64_t needle);
int64_t rt_lists_index_of_i64(void *list, int64_t needle);
void *rt_lists_reverse(void *list);
void *rt_lists_unique(void *list);
int64_t rt_lists_first(void *list, int64_t line, int64_t col, const char *file);
int64_t rt_lists_last(void *list, int64_t line, int64_t col, const char *file);
char *rt_lists_join_list(void *list, const char *sep);

// Vecs module aliases (rt_vecs_*)
int64_t rt_vecs_len(void *vec);
int64_t rt_vecs_get_i64(void *vec, int64_t index);
void   *rt_vecs_get_ptr(void *vec, int64_t index);
void   *rt_vecs_push_i64(void *vec, int64_t value);
void   *rt_vecs_push_ptr(void *vec, void *value);
int64_t rt_vecs_pop(void *vec);
void   *rt_vecs_slice(void *vec, int64_t start, int64_t end);
void   *rt_vecs_concat(void *a, void *b);
void   *rt_vecs_remove(void *vec, int64_t index);
void   *rt_vecs_clear(void *vec);
void   *rt_vecs_flatten(void *vec);
void   *rt_vecs_sort(void *vec);
void   *rt_vecs_reverse(void *vec);
void   *rt_vecs_unique(void *vec);
int64_t rt_vecs_first(void *vec, int64_t line, int64_t col, const char *file);
int64_t rt_vecs_last(void *vec, int64_t line, int64_t col, const char *file);
int64_t rt_vecs_contains_i64(void *vec, int64_t needle);
int64_t rt_vecs_index_of_i64(void *vec, int64_t needle);

// Lists module aliases (rt_lists_*) — backward compat
int64_t rt_lists_len(void *list);
int64_t rt_lists_get_i64(void *list, int64_t index);
void   *rt_lists_get_ptr(void *list, int64_t index);
char   *rt_vec_get_str(void *list, int64_t index);
int64_t rt_vec_len(void *list);
void   *rt_lists_push_i64(void *list, int64_t value);
void   *rt_lists_push_ptr(void *list, void *value);
int64_t rt_lists_pop(void *list);
void   *rt_lists_slice(void *list, int64_t start, int64_t end);
void   *rt_lists_concat(void *a, void *b);
void   *rt_lists_remove(void *list, int64_t index);
void   *rt_lists_clear(void *list);

// ═══════════════════════════════════════════════════════════════════════
//  4. Maps — hash maps (map[K V])
//
//  Implementation split:
//    maps_internal.c — hashing, slot access, bucket management
//    maps.c          — public API (get/set/has/remove/to_string)
//
//  Key/value kinds:
//    MIRE_KIND_SCALAR=1  MIRE_KIND_BOOL=2   MIRE_KIND_STR=3
//    MIRE_KIND_MAP=4     MIRE_KIND_PTR=5
// ═══════════════════════════════════════════════════════════════════════

enum {
    MIRE_KIND_SCALAR = 1,
    MIRE_KIND_BOOL = 2,
    MIRE_KIND_STR = 3,
    MIRE_KIND_MAP = 4,
    MIRE_KIND_PTR = 5,
};

typedef struct MireDictEntry {
    int64_t hash;
    int64_t next;
    int64_t key_i64;
    char *key_str;
} MireDictEntry;

typedef struct {
    int64_t len;
    int64_t cap;
    int64_t key_kind;
    int64_t value_kind;
    int64_t key_size;
    int64_t value_size;
    int64_t bucket_cap;
    int64_t *buckets;
    MireDictEntry *entries;
    uint8_t *key_storage;
    uint8_t *value_storage;
} MireDict;

int64_t rt_dict_len(void *dict_ptr);
void *rt_dict_ensure(void *dict_ptr);
void *rt_dict_ensure_kind(void *dict_ptr, int64_t key_kind, int64_t value_kind);
int64_t rt_dict_get_i64(void *dict_ptr, int64_t key_kind, int64_t key_i64,
                         void *key_ptr, int64_t default_value);
void  *rt_dict_set_i64(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                         int64_t key_i64, void *key_ptr, int64_t value);
void  *rt_dict_get_ptr(void *dict_ptr, int64_t key_kind, int64_t key_i64,
                         void *key_ptr, void *default_value);
void  *rt_dict_set_ptr(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                         int64_t key_i64, void *key_ptr, void *value);
int64_t rt_dict_has(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr);
void  *rt_dict_remove(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr);
char  *rt_dict_to_string(void *dict_ptr);
void   rt_dict_free(void *dict_ptr);
void  *rt_dict_keys(void *dict_ptr);
void  *rt_dict_values(void *dict_ptr);

// Dicts module aliases (rt_dicts_*) — backward compat
int64_t rt_dicts_len(void *dict);
void   *rt_dicts_get(void *dict, const char *key);
int64_t rt_dicts_get_i64(void *dict, const char *key);
void   *rt_dicts_set(void *dict, const char *key, void *value);
void   *rt_dicts_set_i64(void *dict, const char *key, int64_t value);
int64_t rt_dicts_has(void *dict, const char *key);
void   *rt_dicts_remove(void *dict, const char *key);
void   *rt_dicts_keys(void *dict);
void   *rt_dicts_values(void *dict);
int64_t rt_dicts_entries(void *dict);
void   *rt_dicts_merge(void *a, void *b);
int64_t rt_dicts_is_empty(void *dict);
void   *rt_dicts_set_with_kind(void *dict, const char *key, void *value, int64_t value_kind);

// Maps module aliases (rt_maps_*)
int64_t rt_maps_len(void *map);
void   *rt_maps_get(void *map, const char *key);
int64_t rt_maps_get_i64(void *map, const char *key);
void   *rt_maps_set(void *map, const char *key, void *value);
void   *rt_maps_set_i64(void *map, const char *key, int64_t value);
void   *rt_maps_set_with_kind(void *map, const char *key, void *value, int64_t value_kind);
int64_t rt_maps_has(void *map, const char *key);
void   *rt_maps_remove(void *map, const char *key);
void   *rt_maps_keys(void *map);
void   *rt_maps_values(void *map);
int64_t rt_maps_entries(void *map);
void   *rt_maps_merge(void *a, void *b);
int64_t rt_maps_is_empty(void *map);

// ═══════════════════════════════════════════════════════════════════════
//  5. Maybe[T] — optional values
//
//  Representation: NULL = None, non-NULL = heap pointer to value.
//  Unwrap functions panic with location info on None.
//  Unwrap_or functions return the default on None (no panic).
// ═══════════════════════════════════════════════════════════════════════

void *rt_maybe_some_i64(int64_t value);
void *rt_maybe_some_str(char *value);
void *rt_maybe_some_f64(double value);
void *rt_maybe_some_ptr(void *value);
int64_t rt_maybe_is_none(void *ptr);
int64_t rt_maybe_is_some(void *ptr);
void   *rt_maybe_none_as_ptr(void);
int64_t rt_maybe_unwrap_i64(void *ptr, int64_t line, int64_t col, const char *file);
char   *rt_maybe_unwrap_str(void *ptr, int64_t line, int64_t col, const char *file);
double  rt_maybe_unwrap_f64(void *ptr, int64_t line, int64_t col, const char *file);
void   *rt_maybe_unwrap_ptr(void *ptr, int64_t line, int64_t col, const char *file);
int64_t rt_maybe_unwrap_or_i64(void *ptr, int64_t default_val);
char   *rt_maybe_unwrap_or_str(void *ptr, char *default_val);
double  rt_maybe_unwrap_or_f64(void *ptr, double default_val);
void   *rt_maybe_unwrap_or_ptr(void *ptr, void *default_val);
void    rt_maybe_free(void *ptr);

// ═══════════════════════════════════════════════════════════════════════
//  6. Result[T E] — error handling
//
//  Representation: heap-allocated MireResult { tag, payload }.
//  tag=1 → Ok, tag=0 → Err.
//  The ? operator calls rt_result_is_err/rt_maybe_is_none to check,
//  then either propagates (early return) or unwraps.
// ═══════════════════════════════════════════════════════════════════════

void *rt_result_ok_i64(int64_t value);
void *rt_result_ok_str(char *value);
void *rt_result_ok_ptr(void *value);
void *rt_result_err_i64(int64_t error);
void *rt_result_err_str(char *error);
void *rt_result_err_ptr(void *error);
int64_t rt_result_is_ok(void *ptr);
int64_t rt_result_is_err(void *ptr);
void   *rt_result_err_payload(void *ptr);
int64_t rt_result_unwrap_i64(void *ptr, int64_t line, int64_t col, const char *file);
char   *rt_result_unwrap_str(void *ptr, int64_t line, int64_t col, const char *file);
double  rt_result_unwrap_f64(void *ptr, int64_t line, int64_t col, const char *file);
void   *rt_result_unwrap_ptr(void *ptr, int64_t line, int64_t col, const char *file);
char   *rt_result_unwrap_err_str(void *ptr, int64_t line, int64_t col, const char *file);
int64_t rt_result_unwrap_or_i64(void *ptr, int64_t default_val);
char   *rt_result_unwrap_or_str(void *ptr, char *default_val);
double  rt_result_unwrap_or_f64(void *ptr, double default_val);
void   *rt_result_unwrap_or_ptr(void *ptr, void *default_val);
void    rt_result_free(void *ptr);

// ═══════════════════════════════════════════════════════════════════════
//  7. Arr[T N] — fixed-size arrays
//
//  Pointer to N contiguous elements. Count is passed explicitly.
//  No allocation needed; points to stack or static memory.
// ═══════════════════════════════════════════════════════════════════════

int64_t rt_arr_len(void *arr, int64_t count);
int64_t rt_arr_first_i64(void *arr, int64_t count, int64_t line, int64_t col, const char *file);
int64_t rt_arr_last_i64(void *arr, int64_t count, int64_t line, int64_t col, const char *file);
int64_t rt_arr_contains_i64(void *arr, int64_t count, int64_t needle);
int64_t rt_arr_index_of_i64(void *arr, int64_t count, int64_t needle);
void   rt_arr_reverse_i64(void *arr, int64_t count);
char   *rt_arr_join(void *arr, int64_t count, const char *sep);

// ═══════════════════════════════════════════════════════════════════════
//  8. Math operations
// ═══════════════════════════════════════════════════════════════════════

double  rt_math_pi(void);
double  rt_math_e(void);
double  rt_math_tau(void);
double  rt_math_sin(double value);
double  rt_math_cos(double value);
double  rt_math_tan(double value);
double  rt_math_sqrt(double value);
double  rt_math_pow(double base, double exponent);
double  rt_math_log(double value);
double  rt_math_log10(double value);
double  rt_math_exp(double value);
double  rt_math_atan2(double y, double x);
double  rt_math_asin(double value);
double  rt_math_acos(double value);
int64_t rt_math_round(double value);
int64_t rt_math_floor(double value);
int64_t rt_math_ceil(double value);
int64_t rt_math_sum_i64(void *list);
int64_t rt_math_min_list_i64(void *list);
int64_t rt_math_max_list_i64(void *list);
double  rt_math_mean_i64(void *list);
double  rt_math_variance_i64(void *list);
double  rt_math_stddev_i64(void *list);
double  rt_math_median_i64(void *list);
void   *rt_math_range_i64(int64_t end);
void   *rt_math_range_between_i64(int64_t start, int64_t end);
void   *rt_math_range_step_i64(int64_t start, int64_t end, int64_t step);

// Pseudo-random number generation (xorshift* PRNG)
void rt_math_random_seed(int64_t seed);
int64_t rt_math_random_u64(void);
int64_t rt_math_random_i64(void);
double  rt_math_random_f64(void);
int64_t rt_math_random_bool(void);
int64_t rt_math_random_range_i64(int64_t min, int64_t max);

// ═══════════════════════════════════════════════════════════════════════
//  9. Safety — panics, checked arithmetic, bounds checking
//
//  All functions that can fail receive (line, col, file) for
//  error reporting with source location.
// ═══════════════════════════════════════════════════════════════════════

void rt_panic(const char *message);
void rt_panic_loc(const char *msg, int64_t line, int64_t col, const char *file);
int64_t rt_div_i64(int64_t a, int64_t b, int64_t line, int64_t col, const char *file);
int64_t rt_rem_i64(int64_t a, int64_t b, int64_t line, int64_t col, const char *file);
void rt_check_bounds_i64(int64_t index, int64_t len, int64_t line, int64_t col, const char *file);
void *rt_closure_env_alloc(int64_t size);
void  rt_closure_env_free(void *env);

// ═══════════════════════════════════════════════════════════════════════
//  10. Crypto operations
//
//  Pure C implementations. No external dependencies.
// ═══════════════════════════════════════════════════════════════════════

char   *rt_crypto_sha256(const char *data, int64_t len);
char   *rt_crypto_sha256_hex(const char *data, int64_t len);
char   *rt_crypto_hmac_sha256(const char *key, int64_t key_len, const char *data, int64_t data_len);
char   *rt_crypto_hmac_sha256_hex(const char *key, int64_t key_len, const char *data, int64_t data_len);
char   *rt_crypto_base64_encode(const char *data, int64_t len);
char   *rt_crypto_base64_decode(const char *data, int64_t len);
char   *rt_crypto_hex_encode(const char *data, int64_t len);
char   *rt_crypto_hex_decode(const char *hex, int64_t len);
int64_t rt_crypto_byte_at(const char *s, int64_t i);
char   *rt_read_bytes(const char *path);
int      rt_hex_to_file(const char *path, const char *hex);

// ═══════════════════════════════════════════════════════════════════════
//  11. I/O helpers
// ═══════════════════════════════════════════════════════════════════════

void   *dasu(int64_t value);
char   *ireru(const char *prompt);
void   *rt_get_args(int argc, char **argv);
char   *rt_time_elapsed_ms_str(int64_t start_ns);
char   *rt_cpu_elapsed_ms_str(int64_t start_ns);

// ═══════════════════════════════════════════════════════════════════════
//  12. Thread operations
// ═══════════════════════════════════════════════════════════════════════

int64_t rt_thread_spawn_closure(void *fn_ptr, void *env_ptr);

#endif // MIRE_RUNTIME_H
