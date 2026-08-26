#include "runtime.h"
#include "types.h"
#include <stdlib.h>
#include <string.h>

// ═══════════════════════════════════════════════════════════════════════
//  Maybe[T] — NULL=None, non-NULL=pointer to heap-allocated value
//
//  The compiler codegens Maybe as ptr in LLVM IR. To make these inline
//  (no heap), the compiler's llvm_type_str for DataType::Maybe must emit
//  {i64, i64} instead of ptr. That's a future refactor.
//
//  For now: every Some(v) allocates; every matching path must free.
// ═══════════════════════════════════════════════════════════════════════

void *rt_maybe_some_i64(int64_t value) {
    int64_t *slot = (int64_t *)malloc(sizeof(int64_t));
    if (!slot) return NULL;
    *slot = value;
    return (void *)slot;
}

void *rt_maybe_some_str(char *value) {
    char **slot = (char **)malloc(sizeof(char *));
    if (!slot) return NULL;
    *slot = value;
    return (void *)slot;
}

void *rt_maybe_some_f64(double value) {
    double *slot = (double *)malloc(sizeof(double));
    if (!slot) return NULL;
    *slot = value;
    return (void *)slot;
}

void *rt_maybe_some_ptr(void *value) {
    void **slot = (void **)malloc(sizeof(void *));
    if (!slot) return NULL;
    *slot = value;
    return (void *)slot;
}

int64_t rt_maybe_is_none(void *ptr) {
    return ptr == NULL;
}

int64_t rt_maybe_is_some(void *ptr) {
    return ptr != NULL;
}

int64_t rt_maybe_unwrap_i64(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL) rt_panic_loc("called unwrap() on None", line, col, file);
    return *(int64_t *)ptr;
}

char *rt_maybe_unwrap_str(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL) rt_panic_loc("called unwrap() on None", line, col, file);
    return *(char **)ptr;
}

int64_t rt_maybe_unwrap_or_i64(void *ptr, int64_t default_val) {
    if (ptr == NULL) return default_val;
    return *(int64_t *)ptr;
}

double rt_maybe_unwrap_f64(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL) rt_panic_loc("called unwrap() on None", line, col, file);
    return *(double *)ptr;
}

char *rt_maybe_unwrap_or_str(void *ptr, char *default_val) {
    if (ptr == NULL) return default_val;
    return *(char **)ptr;
}

double rt_maybe_unwrap_or_f64(void *ptr, double default_val) {
    if (ptr == NULL) return default_val;
    return *(double *)ptr;
}

void *rt_maybe_unwrap_ptr(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL) rt_panic_loc("called unwrap() on None", line, col, file);
    return *(void **)ptr;
}

void *rt_maybe_unwrap_or_ptr(void *ptr, void *default_val) {
    if (ptr == NULL) return default_val;
    return *(void **)ptr;
}

void rt_maybe_free(void *ptr) {
    if (ptr) free(ptr);
}

// ═══════════════════════════════════════════════════════════════════════
//  Result[T E] — ptr to MireResult {tag, payload}
//  tag=1 → Ok, tag=0 → Err
//
//  Closer to inline than Maybe: the struct is contiguous (16 bytes),
//  allocated as a single block. Still on heap because compiler emits ptr.
// ═══════════════════════════════════════════════════════════════════════

void *rt_result_ok_i64(int64_t value) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 1;
    r->payload = value;
    return (void *)r;
}

void *rt_result_ok_str(char *value) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 1;
    r->payload = (int64_t)(intptr_t)value;
    return (void *)r;
}

void *rt_result_ok_ptr(void *value) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 1;
    r->payload = (int64_t)(intptr_t)value;
    return (void *)r;
}

void *rt_result_err_i64(int64_t error) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 0;
    r->payload = error;
    return (void *)r;
}

void *rt_result_err_str(char *error) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 0;
    r->payload = (int64_t)(intptr_t)error;
    return (void *)r;
}

void *rt_result_err_ptr(void *error) {
    MireResult *r = (MireResult *)malloc(sizeof(MireResult));
    if (!r) return NULL;
    r->tag = 0;
    r->payload = (int64_t)(intptr_t)error;
    return (void *)r;
}

int64_t rt_result_is_ok(void *ptr) {
    if (ptr == NULL) return 0;
    return ((MireResult *)ptr)->tag == 1;
}

int64_t rt_result_is_err(void *ptr) {
    if (ptr == NULL) return 1;
    return ((MireResult *)ptr)->tag == 0;
}

void *rt_result_err_payload(void *ptr) {
    return ptr;
}

void *rt_maybe_none_as_ptr(void) {
    return NULL;
}

int64_t rt_result_unwrap_i64(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0)
        rt_panic_loc("called unwrap() on Err", line, col, file);
    return ((MireResult *)ptr)->payload;
}

char *rt_result_unwrap_str(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0)
        rt_panic_loc("called unwrap() on Err", line, col, file);
    return (char *)(intptr_t)((MireResult *)ptr)->payload;
}

double rt_result_unwrap_f64(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0)
        rt_panic_loc("called unwrap() on Err", line, col, file);
    double val;
    memcpy(&val, &((MireResult *)ptr)->payload, sizeof(double));
    return val;
}

void *rt_result_unwrap_ptr(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0)
        rt_panic_loc("called unwrap() on Err", line, col, file);
    return (void *)(intptr_t)((MireResult *)ptr)->payload;
}

char *rt_result_unwrap_err_str(void *ptr, int64_t line, int64_t col, const char *file) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 1)
        rt_panic_loc("called unwrap_err() on Ok", line, col, file);
    return (char *)(intptr_t)((MireResult *)ptr)->payload;
}

int64_t rt_result_unwrap_or_i64(void *ptr, int64_t default_val) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0) return default_val;
    return ((MireResult *)ptr)->payload;
}

char *rt_result_unwrap_or_str(void *ptr, char *default_val) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0) return default_val;
    return (char *)(intptr_t)((MireResult *)ptr)->payload;
}

double rt_result_unwrap_or_f64(void *ptr, double default_val) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0) return default_val;
    return *(double *)&((MireResult *)ptr)->payload;
}

void *rt_result_unwrap_or_ptr(void *ptr, void *default_val) {
    if (ptr == NULL || ((MireResult *)ptr)->tag == 0) return default_val;
    return (void *)(intptr_t)((MireResult *)ptr)->payload;
}

void rt_result_free(void *ptr) {
    if (ptr) free(ptr);
}

// ═══════════════════════════════════════════════════════════════════════
//  Arr[T N] — pointer to N contiguous elements, count passed explicitly
//
//  Arr is a compile-time sized slice. The pointer points to stack or
//  static memory; no allocation needed. Library functions receive
//  (pointer, count) as separate arguments.
// ═══════════════════════════════════════════════════════════════════════

int64_t rt_arr_len(void *arr, int64_t count) {
    (void)arr;
    return count;
}

int64_t rt_arr_first_i64(void *arr, int64_t count, int64_t line, int64_t col, const char *file) {
    if (count <= 0) rt_panic_loc("called first() on empty array", line, col, file);
    return ((int64_t *)arr)[0];
}

int64_t rt_arr_last_i64(void *arr, int64_t count, int64_t line, int64_t col, const char *file) {
    if (count <= 0) rt_panic_loc("called last() on empty array", line, col, file);
    return ((int64_t *)arr)[count - 1];
}

int64_t rt_arr_contains_i64(void *arr, int64_t count, int64_t needle) {
    int64_t *data = (int64_t *)arr;
    for (int64_t i = 0; i < count; i++) {
        if (data[i] == needle) return 1;
    }
    return 0;
}

int64_t rt_arr_index_of_i64(void *arr, int64_t count, int64_t needle) {
    int64_t *data = (int64_t *)arr;
    for (int64_t i = 0; i < count; i++) {
        if (data[i] == needle) return i;
    }
    return -1;
}

void rt_arr_reverse_i64(void *arr, int64_t count) {
    int64_t *data = (int64_t *)arr;
    for (int64_t i = 0, j = count - 1; i < j; i++, j--) {
        int64_t tmp = data[i];
        data[i] = data[j];
        data[j] = tmp;
    }
}

char *rt_arr_join(void *arr, int64_t count, const char *sep) {
    if (count <= 0) return rt_managed_from_cstr("");
    int64_t *data = (int64_t *)arr;
    char *result = rt_i64_to_string(data[0]);
    for (int64_t i = 1; i < count; i++) {
        char *num = rt_i64_to_string(data[i]);
        char *tmp = rt_string_concat(result, sep);
        rt_managed_free(result);
        result = tmp;
        char *tmp2 = rt_string_concat(result, num);
        rt_managed_free(result);
        rt_managed_free(num);
        result = tmp2;
    }
    return result;
}
