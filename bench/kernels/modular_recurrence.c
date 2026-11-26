/* A loop-carried integer recurrence with bounded arithmetic. */
#include "_common.h"

int main(void) {
    int64_t state = SEED;
    for (int64_t i = 0; i < 12000000; i++) {
        state = (state * 48271 + i + 1) % 2147483647;
    }
    printf("%lld\n", (long long)state);
    return 0;
}
