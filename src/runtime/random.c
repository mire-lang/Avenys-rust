#include "runtime.h"
#include <stdint.h>

static __thread uint64_t g_math_random_state = 0;

static uint64_t next_u64(void) {
    uint64_t x = g_math_random_state;
    if (x == 0) {
        // Seed from address of stack variable + time — not cryptographic,
        // but sufficient to decorrelate threads without external deps.
        x = 0x9e3779b97f4a7c15ULL ^ (uint64_t)(uintptr_t)&x;
    }
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    g_math_random_state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

void rt_math_random_seed(int64_t seed) {
    uint64_t value = (uint64_t)seed;
    g_math_random_state = value ? value : 0x9e3779b97f4a7c15ULL;
}

int64_t rt_math_random_u64(void) {
    return (int64_t)next_u64();
}

int64_t rt_math_random_i64(void) {
    return (int64_t)next_u64();
}

double rt_math_random_f64(void) {
    uint64_t value = next_u64() >> 11;
    return (double)value / 9007199254740992.0;
}

int64_t rt_math_random_bool(void) {
    return (int64_t)(next_u64() & 1ULL);
}

int64_t rt_math_random_range_i64(int64_t min, int64_t max) {
    if (max <= min) return min;
    uint64_t span = (uint64_t)(max - min);
    uint64_t value = next_u64() % span;
    return min + (int64_t)value;
}
