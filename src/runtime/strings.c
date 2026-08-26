#define _POSIX_C_SOURCE 200809L

#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

// ── Internal helpers ──────────────────────────────────────────────────

static size_t str_byte_len(const char *s) {
    if (s == NULL) return 0;
    // Only read the header for strings confirmed to be in the managed table
    if (rt_managed_is_managed(s)) {
        MireManagedString *hdr = rt_string_header(s);
        return hdr->len;
    }
    return strlen(s);
}

char *rt_string_copy(const char *value) {
    if (value == NULL) return rt_managed_from_slice("", 0);
    return rt_managed_from_slice(value, str_byte_len(value));
}

char *rt_string_concat(const char *left, const char *right) {
    if (left == NULL) left = "";
    if (right == NULL) right = "";
    size_t llen = str_byte_len(left);
    size_t rlen = str_byte_len(right);
    char *out = rt_managed_alloc(llen + rlen);
    if (out == NULL) return rt_managed_from_slice("", 0);
    memcpy(out, left, llen);
    memcpy(out + llen, right, rlen);
    out[llen + rlen] = '\0';
    return out;
}

int64_t rt_strings_char_at(const char *s, int64_t index) {
    if (!s || index < 0) return 0;
    size_t len = str_byte_len(s);
    if ((size_t)index >= len) return 0;
    return (unsigned char)s[index];
}

char *rt_strings_repeat(const char *input, int64_t count) {
    if (!input || count <= 0) return rt_managed_from_slice("", 0);
    size_t len = str_byte_len(input);
    if (len == 0) return rt_managed_from_slice("", 0);
    if (count == 1) return rt_managed_from_slice(input, len);

    size_t repeat_count = (size_t)count;
    if (len > SIZE_MAX / repeat_count) return rt_managed_from_slice("", 0);
    size_t total = len * repeat_count;
    char *out = rt_managed_alloc(total);
    if (!out) return rt_managed_from_slice("", 0);
    for (size_t i = 0; i < repeat_count; i++) memcpy(out + (i * len), input, len);
    out[total] = '\0';
    return out;
}

char *rt_string_append_owned(char *value, const char *suffix) {
    if (value == NULL) return rt_string_copy(suffix);
    if (suffix == NULL) return value;
    size_t vlen = str_byte_len(value);
    size_t slen = str_byte_len(suffix);
    if (rt_managed_is_managed(value)) {
        MireManagedString *hdr = rt_string_header(value);
        size_t needed = vlen + slen;
        if (hdr->cap >= needed) {
            memcpy(value + vlen, suffix, slen);
            value[needed] = '\0';
            hdr->len = needed;
            hdr->flags &= ~MIRE_STR_UTF8_KNOWN; // invalidate UTF-8 cache
            return value;
        }
    }
    char *result = rt_managed_alloc(vlen + slen);
    if (result == NULL) {
        char *fallback = rt_string_concat(value, suffix);
        if (!rt_managed_is_managed(value)) free(value);
        return fallback;
    }
    if (vlen > 0) memcpy(result, value, vlen);
    if (slen > 0) memcpy(result + vlen, suffix, slen);
    result[vlen + slen] = '\0';
    if (rt_managed_is_managed(value)) {
        rt_managed_free(value);
    } else {
        free(value);
    }
    return result;
}

char *rt_i64_to_string(int64_t value) {
    return rt_managed_printf_i64("%lld", (long long)value);
}

char *rt_bool_to_string(int64_t value) {
    return rt_managed_from_slice(value ? "true" : "false", value ? 4 : 5);
}

char *rt_f64_to_string(double value) {
    return rt_managed_printf_f64("%g", value);
}

char *rt_f32_to_string(float value) {
    return rt_managed_printf_f64("%g", (double)value);
}

char *rt_i128_to_string(__int128 value) {
    char buf[48];
    int n = 0;
    unsigned __int128 u;
    int neg = 0;
    if (value < 0) {
        neg = 1;
        u = (unsigned __int128)(-(value + 1)) + 1;
    } else {
        u = (unsigned __int128)value;
    }
    if (u == 0) {
        buf[n++] = '0';
    } else {
        while (u > 0) {
            buf[n++] = (char)('0' + (int)(u % 10));
            u /= 10;
        }
    }
    if (neg) buf[n++] = '-';
    for (int i = 0; i < n / 2; i++) {
        char tmp = buf[i];
        buf[i] = buf[n - 1 - i];
        buf[n - 1 - i] = tmp;
    }
    buf[n] = '\0';
    return rt_managed_from_slice(buf, n);
}

char *rt_u128_to_string(unsigned __int128 value) {
    char buf[48];
    int n = 0;
    if (value == 0) {
        buf[n++] = '0';
    } else {
        while (value > 0) {
            buf[n++] = (char)('0' + (int)(value % 10));
            value /= 10;
        }
    }
    for (int i = 0; i < n / 2; i++) {
        char tmp = buf[i];
        buf[i] = buf[n - 1 - i];
        buf[n - 1 - i] = tmp;
    }
    buf[n] = '\0';
    return rt_managed_from_slice(buf, n);
}

char rt_unicode_to_lower(unsigned char c) {
    if (c >= 'A' && c <= 'Z') return (char)(c + 32);
    if (c >= 0xC0 && c <= 0xD6) return (char)(c + 32);
    if (c >= 0xD8 && c <= 0xDE) return (char)(c + 32);
    return (char)c;
}

char rt_unicode_to_upper(unsigned char c) {
    if (c >= 'a' && c <= 'z') return (char)(c - 32);
    if (c >= 0xE0 && c <= 0xF6) return (char)(c - 32);
    if (c >= 0xF8 && c <= 0xFE) return (char)(c - 32);
    return (char)c;
}

char *rt_string_to_upper(const char *value) {
    if (value == NULL) return rt_managed_from_slice("", 0);
    size_t len = str_byte_len(value);
    char *out = rt_managed_alloc(len);
    if (out == NULL) return rt_managed_from_slice("", 0);
    for (size_t i = 0; i < len; i++) {
        out[i] = rt_unicode_to_upper((unsigned char)value[i]);
    }
    out[len] = '\0';
    return out;
}

char *rt_string_to_lower(const char *value) {
    if (value == NULL) return rt_managed_from_slice("", 0);
    size_t len = str_byte_len(value);
    char *out = rt_managed_alloc(len);
    if (out == NULL) return rt_managed_from_slice("", 0);
    for (size_t i = 0; i < len; i++) {
        out[i] = rt_unicode_to_lower((unsigned char)value[i]);
    }
    out[len] = '\0';
    return out;
}

// ── Extended string operations ────────────────────────────────────────

int64_t rt_strings_contains(const char *input, const char *needle) {
    if (!input || !needle) return 0;
    return strstr(input, needle) != NULL ? 1 : 0;
}

char *rt_strings_replace(const char *input, const char *from, const char *to) {
    if (!input) input = "";
    if (!from || !*from) return rt_managed_from_slice(input, str_byte_len(input));
    if (!to) to = "";
    const char *pos = strstr(input, from);
    if (!pos) return rt_managed_from_slice(input, str_byte_len(input));
    size_t prefix_len = (size_t)(pos - input);
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    size_t suffix_len = str_byte_len(pos + from_len);
    char *out = rt_managed_alloc(prefix_len + to_len + suffix_len);
    if (!out) return rt_managed_from_slice("", 0);
    memcpy(out, input, prefix_len);
    memcpy(out + prefix_len, to, to_len);
    memcpy(out + prefix_len + to_len, pos + from_len, suffix_len);
    out[prefix_len + to_len + suffix_len] = '\0';
    return out;
}

char *rt_strings_replace_first(const char *input, const char *from, const char *to) {
    if (!input || !from || !*from) return rt_managed_from_slice(input ? input : "", input ? str_byte_len(input) : 0);
    if (!to) to = "";
    const char *pos = strstr(input, from);
    if (!pos) return rt_managed_from_slice(input, str_byte_len(input));
    size_t prefix = (size_t)(pos - input);
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    size_t suffix = str_byte_len(pos + from_len);
    char *out = rt_managed_alloc(prefix + to_len + suffix);
    if (!out) return rt_managed_from_slice("", 0);
    memcpy(out, input, prefix);
    memcpy(out + prefix, to, to_len);
    memcpy(out + prefix + to_len, pos + from_len, suffix);
    out[prefix + to_len + suffix] = '\0';
    return out;
}

int64_t rt_strings_starts_with(const char *str, const char *prefix) {
    if (!str || !prefix) return 0;
    size_t slen = str_byte_len(str), plen = strlen(prefix);
    if (plen > slen) return 0;
    return strncmp(str, prefix, plen) == 0 ? 1 : 0;
}

int64_t rt_strings_ends_with(const char *str, const char *suffix) {
    if (!str || !suffix) return 0;
    size_t slen = str_byte_len(str), suflen = strlen(suffix);
    if (suflen > slen) return 0;
    return strcmp(str + slen - suflen, suffix) == 0 ? 1 : 0;
}

char *rt_strings_substr(const char *input, int64_t start, int64_t length) {
    if (!input) return rt_managed_from_slice("", 0);
    size_t len = str_byte_len(input);
    if (start < 0) start = 0;
    if ((size_t)start >= len || length <= 0) return rt_managed_from_slice("", 0);
    int64_t avail = (int64_t)(len - (size_t)start);
    if (length > avail) length = avail;
    return rt_managed_from_slice(input + start, (size_t)length);
}

static char *pad_core(const char *input, int64_t width, const char *pad, int left_pad) {
    if (!input) input = "";
    if (!pad) pad = " ";
    size_t ilen = str_byte_len(input);
    size_t plen = strlen(pad);
    if (plen == 0 || (int64_t)ilen >= width) return rt_managed_from_slice(input, ilen);
    size_t pad_total = (size_t)(width - (int64_t)ilen);
    size_t repeats = pad_total / plen + 1;
    size_t buf_len = repeats * plen;
    char *pad_buf = (char *)malloc(buf_len);
    if (!pad_buf) return rt_managed_from_slice(input, ilen);
    for (size_t i = 0; i < repeats; i++) memcpy(pad_buf + i * plen, pad, plen);
    size_t result_len = (size_t)width;
    char *out = rt_managed_alloc(result_len);
    if (!out) { free(pad_buf); return rt_managed_from_slice("", 0); }
    if (left_pad) {
        memcpy(out, pad_buf, pad_total);
        memcpy(out + pad_total, input, ilen);
    } else {
        memcpy(out, input, ilen);
        memcpy(out + ilen, pad_buf, pad_total);
    }
    out[result_len] = '\0';
    free(pad_buf);
    return out;
}

char *rt_strings_pad_left(const char *input, int64_t width, const char *pad) {
    return pad_core(input, width, pad, 1);
}

char *rt_strings_pad_right(const char *input, int64_t width, const char *pad) {
    return pad_core(input, width, pad, 0);
}

char *rt_strings_trim(const char *input) {
    if (!input) return rt_managed_from_slice("", 0);
    size_t len = str_byte_len(input);
    if (len == 0) return rt_managed_from_slice("", 0);
    const char *start = input;
    const char *end = input + len - 1;
    while (start <= end && (unsigned char)*start <= ' ') start++;
    while (end >= start && (unsigned char)*end <= ' ') end--;
    if (start > end) return rt_managed_from_slice("", 0);
    return rt_managed_from_slice(start, (size_t)(end - start + 1));
}

char *rt_strings_split_list(const char *input, const char *delimiter) {
    if (input == NULL || delimiter == NULL) return (char *)rt_list_create(0, 8);
    size_t delim_len = strlen(delimiter);
    size_t input_len = str_byte_len(input);
    if (delim_len == 0) {
        void *list = rt_list_create(1, 8);
        if (!list) return NULL;
        char *copy = rt_managed_from_cstr(input);
        if (copy) list = rt_list_push_ptr(list, copy);
        return (char *)list;
    }
    size_t count = 1;
    const char *cursor = input;
    const char *match;
    while ((match = strstr(cursor, delimiter)) != NULL) { count++; cursor = match + delim_len; }
    void *list = rt_list_create((int64_t)count, 8);
    if (!list) return NULL;
    const char *seg_start = input;
    cursor = input;
    while ((match = strstr(cursor, delimiter)) != NULL) {
        size_t seg_len = (size_t)(match - seg_start);
        char *copy = rt_managed_from_slice(seg_start, seg_len);
        if (copy) list = rt_list_push_ptr(list, copy);
        seg_start = match + delim_len;
        cursor = seg_start;
    }
    size_t tail_len = input_len - (size_t)(seg_start - input);
    char *tail = rt_managed_from_slice(seg_start, tail_len);
    if (tail) list = rt_list_push_ptr(list, tail);
    return (char *)list;
}

char *rt_strings_join(char **parts, int64_t count, const char *delimiter) {
    if (!parts || count == 0) return rt_managed_from_slice("", 0);
    if (!delimiter) delimiter = "";
    size_t dlen = strlen(delimiter);
    size_t total = 0;
    for (int64_t i = 0; i < count; i++) {
        if (parts[i]) total += str_byte_len(parts[i]);
    }
    total += dlen * (size_t)(count - 1);
    char *out = rt_managed_alloc(total);
    if (!out) return rt_managed_from_slice("", 0);
    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        if (i > 0 && dlen > 0) { memcpy(out + pos, delimiter, dlen); pos += dlen; }
        if (parts[i]) {
            size_t plen = str_byte_len(parts[i]);
            memcpy(out + pos, parts[i], plen);
            pos += plen;
        }
    }
    out[pos] = '\0';
    return out;
}

int64_t rt_string_to_i64(const char *value) {
    if (!value) return 0;
    return (int64_t)atoll(value);
}

void *rt_get_args(int argc, char **argv) {
    void *list = rt_list_create(argc > 0 ? argc : 4, 8);
    for (int i = 0; i < argc; i++) {
        char *copy = rt_strdup_raw(argv[i]);
        list = rt_list_push_ptr(list, copy);
    }
    return list;
}

char **rt_build_argv(const char *cmd, void *args_vec, int64_t *argc_out) {
    if (!cmd || !args_vec || !argc_out) return NULL;
    int64_t nargs = rt_lists_len(args_vec);
    if (nargs < 0 || nargs > (INT64_MAX / (int64_t)sizeof(char *)) - 2) return NULL;
    size_t argc = (size_t)nargs + 2;
    if (argc > SIZE_MAX / sizeof(char *)) return NULL;
    char **argv = (char **)calloc(argc, sizeof(char *));
    if (!argv) return NULL;
    argv[0] = rt_strdup_raw(cmd);
    if (!argv[0]) {
        free(argv);
        return NULL;
    }
    for (int64_t i = 0; i < nargs; i++) {
        argv[i + 1] = rt_strdup_raw(rt_vec_get_str(args_vec, i));
        if (!argv[i + 1]) {
            rt_free_argv(argv, i + 1);
            return NULL;
        }
    }
    argv[nargs + 1] = NULL;
    *argc_out = nargs + 1;
    return argv;
}

void rt_free_argv(char **argv, int64_t argc) {
    if (!argv) return;
    for (int64_t i = 0; i < argc; i++) {
        if (argv[i]) free(argv[i]);
    }
    free(argv);
}

static int64_t monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

char *rt_time_elapsed_ms_str(int64_t start_ns) {
    int64_t ms = (monotonic_ns() - start_ns) / 1000000;
    return rt_managed_printf_i64("%lld", (long long)ms);
}

char *rt_cpu_elapsed_ms_str(int64_t start_ns) {
    int64_t ms = (int64_t)(clock() / (CLOCKS_PER_SEC / 1000)) - start_ns;
    return rt_managed_printf_i64("%lld", (long long)ms);
}

int64_t rt_strings_index_of(const char *s, const char *sub) {
    if (!s || !sub) return -1;
    if (*sub == '\0') return 0;
    const char *pos = strstr(s, sub);
    if (!pos) return -1;
    return (int64_t)(pos - s);
}

char *rt_strings_to_upper(const char *s) { return rt_string_to_upper(s); }
char *rt_strings_to_lower(const char *s) { return rt_string_to_lower(s); }
char *rt_strings_strip(const char *s) { return rt_strings_trim(s); }

void *rt_strings_split(const char *s, const char *sep) {
    void *list = rt_list_create(8, 8);
    if (!s || !sep || !*sep) { list = rt_list_push_ptr(list, rt_managed_from_slice(s ? s : "", s ? str_byte_len(s) : 0)); return list; }
    size_t sep_len = strlen(sep);
    size_t s_len = str_byte_len(s);
    const char *p = s;
    while (*p) {
        const char *found = strstr(p, sep);
        if (!found) {
            list = rt_list_push_ptr(list, rt_managed_from_slice(p, str_byte_len(p)));
            break;
        }
        list = rt_list_push_ptr(list, rt_managed_from_slice(p, found - p));
        p = found + sep_len;
    }
    if (s_len >= sep_len && memcmp(s + s_len - sep_len, sep, sep_len) == 0) {
        list = rt_list_push_ptr(list, rt_managed_from_slice("", 0));
    }
    return list;
}

char *rt_strings_join_list(void *parts, const char *sep) {
    int64_t count = rt_list_len(parts);
    size_t dlen = sep ? strlen(sep) : 0;
    size_t total = 0;
    for (int64_t i = 0; i < count; i++) {
        char *s = rt_list_get_ptr(parts, i);
        if (s) total += str_byte_len(s);
    }
    total += dlen * (count > 0 ? count - 1 : 0);
    char *out = rt_managed_alloc(total);
    if (!out) return rt_managed_from_slice("", 0);
    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        if (i > 0 && dlen > 0) { memcpy(out + pos, sep, dlen); pos += dlen; }
        char *s = rt_list_get_ptr(parts, i);
        if (s) {
            size_t slen = str_byte_len(s);
            memcpy(out + pos, s, slen);
            pos += slen;
        }
    }
    out[pos] = '\0';
    return out;
}
