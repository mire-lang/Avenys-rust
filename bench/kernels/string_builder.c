/* S5: string builder.
 *
 * The C leg repeatedly appends to a dynamically growing buffer.
 */
#include "_common.h"
#include <string.h>

int main(void) {
    int64_t seed = SEED;
    size_t cap = 256;
    size_t len = 0;
    char *s = (char *)malloc(cap);
    s[0] = 0;
    char buf[32];
    for (int64_t i = 0; i < 300; i++) {
        int n = snprintf(buf, sizeof(buf), "%ld", (long)i);
        if (len + n + 1 > cap) {
            cap *= 2;
            s = (char *)realloc(s, cap);
        }
        memcpy(s + len, buf, n);
        len += n;
        s[len] = 0;
    }
    printf("%zu\n", len);
    free(s);
    return 0;
}
