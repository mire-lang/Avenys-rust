#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>

void rt_panic_division_by_zero(void) {
    fprintf(stderr, "division by zero\n");
    exit(1);
}

void rt_panic_out_of_bounds(void) {
    fprintf(stderr, "index out of bounds\n");
    exit(1);
}

int64_t rt_div_i64(int64_t a, int64_t b) {
    if (b == 0) rt_panic_division_by_zero();
    return a / b;
}

int64_t rt_rem_i64(int64_t a, int64_t b) {
    if (b == 0) rt_panic_division_by_zero();
    return a % b;
}

void rt_check_bounds_i64(int64_t index, int64_t len) {
    if (index < 0 || index >= len) rt_panic_out_of_bounds();
}

void *rt_closure_env_alloc(int64_t size) {
    return malloc((size_t)size);
}

void rt_closure_env_free(void *env) {
    if (env) free(env);
}
