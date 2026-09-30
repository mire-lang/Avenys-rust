#include "runtime.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Minimal string operations for freestanding/WASI targets.
 * Excludes functions that require POSIX time APIs (clock_gettime, clock).
 */

char *rt_string_copy(const char *value) {
    if (!value) return NULL;
    size_t len = strlen(value);
    char *copy = malloc(len + 1);
    if (!copy) return NULL;
    memcpy(copy, value, len + 1);
    return copy;
}

char *rt_string_concat(const char *left, const char *right) {
    if (!left) left = "";
    if (!right) right = "";
    size_t ll = strlen(left);
    size_t rl = strlen(right);
    char *result = malloc(ll + rl + 1);
    if (!result) return NULL;
    memcpy(result, left, ll);
    memcpy(result + ll, right, rl + 1);
    return result;
}

int64_t rt_strings_char_at(const char *s, int64_t index) {
    if (!s || index < 0) return -1;
    size_t len = strlen(s);
    if ((size_t)index >= len) return -1;
    return (int64_t)(unsigned char)s[index];
}

char *rt_strings_repeat(const char *input, int64_t count) {
    if (!input || count <= 0) return strdup("");
    size_t len = strlen(input);
    if (len == 0) return strdup("");
    size_t total = len * (size_t)count;
    char *result = malloc(total + 1);
    if (!result) return NULL;
    char *p = result;
    for (int64_t i = 0; i < count; i++) {
        memcpy(p, input, len);
        p += len;
    }
    *p = '\0';
    return result;
}

char *rt_string_append_owned(char *value, const char *suffix) {
    if (!value) return rt_string_copy(suffix);
    if (!suffix) return value;
    size_t vl = strlen(value);
    size_t sl = strlen(suffix);
    char *new_val = realloc(value, vl + sl + 1);
    if (!new_val) return value;
    memcpy(new_val + vl, suffix, sl + 1);
    return new_val;
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
            buf[n++] = '0' + (int)(u % 10);
            u /= 10;
        }
    }
    if (neg) buf[n++] = '-';
    buf[n] = '\0';
    for (int i = 0; i < n / 2; i++) {
        char tmp = buf[i];
        buf[i] = buf[n - 1 - i];
        buf[n - 1 - i] = tmp;
    }
    return rt_string_copy(buf);
}

char *rt_u128_to_string(unsigned __int128 value) {
    char buf[48];
    int n = 0;
    if (value == 0) {
        buf[n++] = '0';
    } else {
        while (value > 0) {
            buf[n++] = '0' + (int)(value % 10);
            value /= 10;
        }
    }
    buf[n] = '\0';
    for (int i = 0; i < n / 2; i++) {
        char tmp = buf[i];
        buf[i] = buf[n - 1 - i];
        buf[n - 1 - i] = tmp;
    }
    return rt_string_copy(buf);
}

char *rt_string_to_upper(const char *value) {
    if (!value) return NULL;
    size_t len = strlen(value);
    char *result = malloc(len + 1);
    if (!result) return NULL;
    for (size_t i = 0; i < len; i++) {
        char c = value[i];
        result[i] = (c >= 'a' && c <= 'z') ? c - 32 : c;
    }
    result[len] = '\0';
    return result;
}

char *rt_string_to_lower(const char *value) {
    if (!value) return NULL;
    size_t len = strlen(value);
    char *result = malloc(len + 1);
    if (!result) return NULL;
    for (size_t i = 0; i < len; i++) {
        char c = value[i];
        result[i] = (c >= 'A' && c <= 'Z') ? c + 32 : c;
    }
    result[len] = '\0';
    return result;
}

int64_t rt_strings_contains(const char *input, const char *needle) {
    if (!input || !needle) return 0;
    return strstr(input, needle) ? 1 : 0;
}

char *rt_strings_replace(const char *input, const char *from, const char *to) {
    if (!input || !from || !to || strlen(from) == 0) return rt_string_copy(input);
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    const char *p = input;
    size_t result_cap = strlen(input) + 1;
    char *result = malloc(result_cap);
    if (!result) return NULL;
    size_t result_len = 0;
    while (*p) {
        if (strncmp(p, from, from_len) == 0) {
            if (result_len + to_len + 1 >= result_cap) {
                result_cap *= 2;
                char *new_result = realloc(result, result_cap);
                if (!new_result) {
                    free(result);
                    return NULL;
                }
                result = new_result;
            }
            memcpy(result + result_len, to, to_len);
            result_len += to_len;
            p += from_len;
        } else {
            if (result_len + 1 >= result_cap) {
                result_cap *= 2;
                char *new_result = realloc(result, result_cap);
                if (!new_result) {
                    free(result);
                    return NULL;
                }
                result = new_result;
            }
            result[result_len++] = *p++;
        }
    }
    result[result_len] = '\0';
    return result;
}

char *rt_strings_replace_first(const char *input, const char *from, const char *to) {
    if (!input || !from || !to || strlen(from) == 0) return rt_string_copy(input);
    const char *found = strstr(input, from);
    if (!found) return rt_string_copy(input);
    size_t prefix_len = found - input;
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    size_t suffix_len = strlen(found + from_len);
    size_t total = prefix_len + to_len + suffix_len;
    char *result = malloc(total + 1);
    if (!result) return NULL;
    memcpy(result, input, prefix_len);
    memcpy(result + prefix_len, to, to_len);
    memcpy(result + prefix_len + to_len, found + from_len, suffix_len + 1);
    return result;
}

int64_t rt_strings_starts_with(const char *str, const char *prefix) {
    if (!str || !prefix) return 0;
    size_t prefix_len = strlen(prefix);
    if (strlen(str) < prefix_len) return 0;
    return strncmp(str, prefix, prefix_len) == 0 ? 1 : 0;
}

int64_t rt_strings_ends_with(const char *str, const char *suffix) {
    if (!str || !suffix) return 0;
    size_t str_len = strlen(str);
    size_t suffix_len = strlen(suffix);
    if (str_len < suffix_len) return 0;
    return strcmp(str + str_len - suffix_len, suffix) == 0 ? 1 : 0;
}

char *rt_strings_substr(const char *input, int64_t start, int64_t length) {
    if (!input || start < 0 || length < 0) return strdup("");
    size_t len = strlen(input);
    if ((size_t)start >= len) return strdup("");
    size_t end = (size_t)start + (size_t)length;
    if (end > len) end = len;
    size_t sub_len = end - (size_t)start;
    char *result = malloc(sub_len + 1);
    if (!result) return NULL;
    memcpy(result, input + start, sub_len);
    result[sub_len] = '\0';
    return result;
}

char *rt_strings_pad_left(const char *input, int64_t width, const char *pad) {
    if (!input) input = "";
    if (!pad) pad = " ";
    int64_t len = rt_strings_len(input);
    if (len >= width) return rt_string_copy(input);
    int64_t pad_count = width - len;
    size_t pad_len = strlen(pad);
    if (pad_len == 0) return rt_string_copy(input);
    size_t total_pad = (size_t)pad_count * pad_len;
    char *result = malloc(total_pad + len + 1);
    if (!result) return NULL;
    char *p = result;
    for (int64_t i = 0; i < pad_count; i++) {
        memcpy(p, pad, pad_len);
        p += pad_len;
    }
    memcpy(p, input, len + 1);
    return result;
}

char *rt_strings_pad_right(const char *input, int64_t width, const char *pad) {
    if (!input) input = "";
    if (!pad) pad = " ";
    int64_t len = rt_strings_len(input);
    if (len >= width) return rt_string_copy(input);
    int64_t pad_count = width - len;
    size_t pad_len = strlen(pad);
    if (pad_len == 0) return rt_string_copy(input);
    size_t total_pad = (size_t)pad_count * pad_len;
    char *result = malloc(len + total_pad + 1);
    if (!result) return NULL;
    memcpy(result, input, len);
    char *p = result + len;
    for (int64_t i = 0; i < pad_count; i++) {
        memcpy(p, pad, pad_len);
        p += pad_len;
    }
    *p = '\0';
    return result;
}

char *rt_strings_trim(const char *input) {
    if (!input) return strdup("");
    size_t len = strlen(input);
    size_t start = 0;
    while (start < len && (input[start] == ' ' || input[start] == '\t' || input[start] == '\n' || input[start] == '\r')) start++;
    size_t end = len;
    while (end > start && (input[end - 1] == ' ' || input[end - 1] == '\t' || input[end - 1] == '\n' || input[end - 1] == '\r')) end--;
    return rt_strings_substr(input, (int64_t)start, (int64_t)(end - start));
}

char *rt_strings_split_list(const char *input, const char *delimiter) {
    return rt_string_copy(input);
}

char *rt_strings_join(char **parts, int64_t count, const char *delimiter) {
    if (!parts || count <= 0) return strdup("");
    if (!delimiter) delimiter = "";
    size_t total = 0;
    for (int64_t i = 0; i < count; i++) {
        if (parts[i]) total += strlen(parts[i]);
    }
    size_t delim_len = strlen(delimiter);
    total += (size_t)(count - 1) * delim_len;
    char *result = malloc(total + 1);
    if (!result) return NULL;
    char *p = result;
    for (int64_t i = 0; i < count; i++) {
        if (parts[i]) {
            size_t len = strlen(parts[i]);
            memcpy(p, parts[i], len);
            p += len;
        }
        if (i < count - 1 && delim_len > 0) {
            memcpy(p, delimiter, delim_len);
            p += delim_len;
        }
    }
    *p = '\0';
    return result;
}

int64_t rt_string_to_i64(const char *value) {
    if (!value) return 0;
    return atoll(value);
}

int64_t rt_f64_to_i64(double value) {
    return (int64_t)value;
}

void rt_free_argv(char **argv, int64_t argc) {
    if (!argv) return;
    for (int64_t i = 0; i < argc; i++) {
        free(argv[i]);
    }
    free(argv);
}

int64_t rt_strings_index_of(const char *s, const char *sub) {
    if (!s || !sub) return -1;
    const char *found = strstr(s, sub);
    return found ? (int64_t)(found - s) : -1;
}

char *rt_strings_join_list(void *parts, const char *sep) {
    int64_t count = rt_list_len(parts);
    size_t dlen = sep ? strlen(sep) : 0;
    size_t total = 0;
    for (int64_t i = 0; i < count; i++) {
        char *s = rt_list_get_ptr(parts, i);
        if (s) total += strlen(s);
    }
    total += dlen * (count > 0 ? count - 1 : 0);
    char *out = rt_managed_alloc(total);
    if (!out) return rt_managed_from_slice("", 0);
    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        if (i > 0 && dlen > 0) { memcpy(out + pos, sep, dlen); pos += dlen; }
        char *s = rt_list_get_ptr(parts, i);
        if (s) {
            size_t slen = strlen(s);
            memcpy(out + pos, s, slen);
            pos += slen;
        }
    }
    out[pos] = '\0';
    return out;
}

/* 
 * Time functions are EXCLUDED from minimal:
 * - rt_time_elapsed_ms_str (uses clock_gettime/clock)
 * - rt_cpu_elapsed_ms_str (uses clock_gettime/clock)
 * 
 * These require POSIX time APIs not available in WASI/freestanding.
 */