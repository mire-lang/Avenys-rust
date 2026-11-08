/* A1: small object allocation pressure.
 *
 * Millions of small allocations (16-64 bytes) followed by frees.
 * C uses malloc/free directly with the same size pattern. */
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    for (int64_t i = 0; i < 1000000; i++) {
        int sz = 16 + (i % 7) * 8;
        int64_t *v = (int64_t *)malloc(sz);
        for (int64_t j = 0; j < sz / 8; j++) {
            v[j] = i + j;
        }
        seed = (seed + (sz / 8) + v[0]) % 1000003;
        free(v);
    }
    printf("%lld\n", (long long)seed);
    return 0;
}