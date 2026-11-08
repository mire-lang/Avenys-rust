/* A4: memory copy benchmark.
 *
 * Copying buffers of different sizes.
 * C uses memcpy directly. */
#include "_common.h"
#include <string.h>

int main(void) {
    int64_t seed = SEED;
    for (int64_t i = 0; i < 100000; i++) {
        int sz = 64 + (i % 16384) * 64;
        int64_t *src = (int64_t *)malloc(sz);
        for (int64_t j = 0; j < sz / 8; j++) {
            src[j] = i + j;
        }
        int64_t *dst = (int64_t *)malloc(sz);
        memcpy(dst, src, sz);
        seed = (seed + (sz / 8) + dst[0]) % 1000003;
        free(src);
        free(dst);
    }
    printf("%lld\n", (long long)seed);
    return 0;
}