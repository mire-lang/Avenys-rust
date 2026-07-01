#include "../pal.h"
#include "runtime.h"
#include <stdio.h>

void pal_io_print(const char *msg) {
    if (msg) printf("%s", msg);
}

void pal_io_print_err(const char *msg) {
    if (msg) fprintf(stderr, "%s", msg);
}

char *pal_io_readln(void) {
    return ireru(NULL);
}
