/* H3: integer hash throughput.
 *
 * Multiply-heavy integer hash chain over an i64, then a modular fold into a
 * checksum. Measures the pure integer hash arithmetic without allocation or
 * containers.
 */
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    uint64_t acc = 0;
    for (int64_t i = 0; i < 20000000; i++) {
        uint64_t z = (uint64_t)i * 2654435761ULL + 1;
        uint64_t h = z * 33 + z * 7;
        acc = acc + (h % 1000003);
    }
    printf("%llu\n", (unsigned long long)acc);
    return 0;
}
