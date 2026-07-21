// WASM PAL — Channel stub
#include "pal.h"
#include "../../runtime/runtime.h"

int64_t pal_channel_create(void) {
    rt_panic("pal_channel_create: not supported in WASM");
    return -1;
}

int64_t pal_channel_send(int64_t write_fd, const char *data) {
    (void)write_fd; (void)data;
    return -1;
}

char *pal_channel_recv(int64_t read_fd) {
    (void)read_fd;
    return rt_strdup_raw("");
}

void pal_channel_close(int64_t fd) {
    (void)fd;
}
