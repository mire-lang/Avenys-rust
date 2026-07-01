#include "runtime.h"
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

void *dasu(int64_t value) {
    printf("%ld\n", value);
    fflush(stdout);
    return NULL;
}

char *ireru(const char *prompt) {
    if (prompt && *prompt) {
        printf("%s", prompt);
        fflush(stdout);
    }
    size_t cap = 128, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return rt_managed_from_slice("", 0);
    int ch;
    while ((ch = getchar()) != EOF && ch != '\n') {
        if (len + 1 >= cap) {
            cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) break;
            buf = nb;
        }
        buf[len++] = (char)ch;
    }
    buf[len] = '\0';
    char *result = rt_managed_from_slice(buf, len);
    free(buf);
    return result;
}
