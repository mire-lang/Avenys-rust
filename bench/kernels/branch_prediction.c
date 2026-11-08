/* D2: branch prediction benchmark.
 *
 * Predictable vs random branches over the same data.
 * C uses simple arrays and loops. */
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    int64_t data[10000];
    for (int64_t i = 0; i < 10000; i++) {
        data[i] = i * 7 + 13;
    }
    // Phase 1: predictable branch
    for (int64_t i = 0; i < 10000; i++) {
        int64_t x = data[i];
        if (i < 5000) {
            seed = (seed + x) % 1000003;
        } else {
            seed = (seed + x * 2) % 1000003;
        }
    }
    // Phase 2: pseudo-random branch
    for (int64_t i = 0; i < 10000; i++) {
        int64_t x = data[i];
        if (x % 3 == 0) {
            seed = (seed + x) % 1000003;
        } else {
            seed = (seed + x * 3) % 1000003;
        }
    }
    printf("%lld\n", (long long)seed);
    return 0;
}