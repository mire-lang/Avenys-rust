/* D1: function call overhead.
 *
 * Millions of direct calls to small functions.
 * C uses simple inline functions. */
#include "_common.h"

__attribute__((noinline)) static int64_t leaf_add(int64_t a, int64_t b) { return a + b; }
__attribute__((noinline)) static int64_t leaf_mul(int64_t a, int64_t b) { return a * b; }

int main(void) {
    int64_t seed = SEED;
    for (int64_t i = 0; i < 5000000; i++) {
        seed = (seed + leaf_add(seed, i)) % 1000003;
        seed = (seed + leaf_mul(seed, i)) % 1000003;
    }
    printf("%lld\n", (long long)seed);
    return 0;
}