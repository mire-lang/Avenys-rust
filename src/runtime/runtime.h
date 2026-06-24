#ifndef MIRE_RUNTIME_H
#define MIRE_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

// ── Managed strings ──────────────────────────────────────────────────
// Format: header (len, cap) + data[cap+1] (inline, no pointer indirection)
typedef struct {
    size_t len;
    size_t cap;
    char data[];
} MireManagedString;

char *rt_managed_alloc(size_t len);
char *rt_managed_from_slice(const char *src, size_t len);
char *rt_managed_from_cstr(const char *src);
char *rt_managed_printf_i64(const char *fmt, long long value);
char *rt_managed_printf_f64(const char *fmt, double value);
void  rt_managed_free(char *value);
void  rt_managed_cleanup_all(void);
size_t rt_managed_len(const char *value);
int   rt_managed_contains(const char *data_ptr);
void  rt_managed_register(char *data_ptr);
void  rt_managed_unregister(char *data_ptr);

// low-level helpers
char *rt_strdup_raw(const char *src);
char *rt_strdup_raw_n(const char *src, size_t len);
char *rt_alloc_printf_raw_i64(const char *fmt, long long value);
size_t rt_string_growth_cap(size_t min_cap);

// ── String operations ────────────────────────────────────────────────
char *rt_string_copy(const char *value);
char *rt_string_concat(const char *left, const char *right);
char *rt_strings_repeat(const char *input, int64_t count);
char *rt_string_append_owned(char *value, const char *suffix);

char *rt_i64_to_string(int64_t value);
char *rt_bool_to_string(int64_t value);
char *rt_f64_to_string(double value);

char *rt_string_to_upper(const char *value);
char *rt_string_to_lower(const char *value);
char  rt_unicode_to_lower(unsigned char c);
char  rt_unicode_to_upper(unsigned char c);

// ── List operations (inline-storage dynamic arrays) ──────────────────
// Format: [capacity, length, data...]
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
void *rt_list_get_ptr(void *list_ptr, int64_t index);

// ── Dict operations (hash table, open addressing) ────────────────────
// Key/value kinds
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

// ── Extended string operations ───────────────────────────────────────
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
int64_t rt_string_to_i64(const char *value);
int64_t rt_strings_index_of(const char *s, const char *sub);

// ── I/O helpers ──────────────────────────────────────────────────────
char   *rt_read_line(const char *prompt);
void   *rt_get_args(int argc, char **argv);
void    dasu_i64(int64_t value);

// ── Time / CPU string formatters ──────────────────────────────────────
char   *rt_time_elapsed_ms_str(int64_t start_ns);
char   *rt_cpu_elapsed_ms_str(int64_t start_ns);

// ── Runtime utilities ────────────────────────────────────────────────
void rt_panic(const char *message);

// ── Strings module aliases (rt_strings_*) ─────────────────────────────
int64_t rt_strings_len(const char *s);
char   *rt_strings_to_upper(const char *s);
char   *rt_strings_to_lower(const char *s);
char   *rt_strings_strip(const char *s);
void   *rt_strings_split(const char *s, const char *sep);
char   *rt_strings_join_list(void *parts, const char *sep);

// ── Lists module aliases (rt_lists_*) ─────────────────────────────────
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
void    rt_list_free(void *list);
void   *rt_lists_flatten(void *list);
void   *rt_lists_sort(void *list);
char   *rt_lists_join_list(void *list, const char *sep);
int64_t rt_lists_first(void *list);
int64_t rt_lists_last(void *list);
int64_t rt_lists_contains_i64(void *list, int64_t needle);
int64_t rt_lists_index_of_i64(void *list, int64_t needle);
void   *rt_lists_reverse(void *list);
void   *rt_lists_unique(void *list);

// ── Dicts module aliases (rt_dicts_*) ─────────────────────────────────
int64_t rt_dicts_len(void *dict);
void   *rt_dicts_get(void *dict, const char *key);
void   *rt_dicts_set(void *dict, const char *key, void *value);
void   *rt_dicts_set_i64(void *dict, const char *key, int64_t value);
int64_t rt_dicts_has(void *dict, const char *key);
void   *rt_dicts_remove(void *dict, const char *key);
void   *rt_dicts_keys(void *dict);
void   *rt_dicts_values(void *dict);
int64_t rt_dicts_entries(void *dict);
void   *rt_dicts_merge(void *a, void *b);
int64_t rt_dicts_is_empty(void *dict);

// ── Math operations ─────────────────────────────────────────────────
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

// ── Runtime safety panics ────────────────────────────────────────────
void rt_panic_division_by_zero(void);
void rt_panic_out_of_bounds(void);
int64_t rt_div_i64(int64_t a, int64_t b);
int64_t rt_rem_i64(int64_t a, int64_t b);
void rt_check_bounds_i64(int64_t index, int64_t len);
void *rt_closure_env_alloc(int64_t size);
void  rt_closure_env_free(void *env);

#endif // MIRE_RUNTIME_H
