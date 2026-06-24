#include <stdio.h>
#include <stdint.h>

void *dasu(int64_t value) {
    printf("%ld\n", value);
    fflush(stdout);
    return NULL;
}
