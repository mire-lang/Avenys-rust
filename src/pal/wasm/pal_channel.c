// WASM PAL — Channel (real single-threaded message queue)
// WASM has no pthreads, so we use a simple ring buffer.
// recv() returns "" if no message is pending (non-blocking).

#include "pal.h"
#include "../../runtime/runtime.h"
#include <stdlib.h>
#include <string.h>

#define CHANNEL_CAP 16
#define CHANNEL_MSG_MAX 4096

typedef struct {
    char *data;
    size_t len;
} ChannelMsg;

typedef struct {
    ChannelMsg msgs[CHANNEL_CAP];
    int head;
    int tail;
    int count;
} Channel;

int64_t pal_channel_create(void) {
    Channel *ch = (Channel *)calloc(1, sizeof(Channel));
    if (!ch) return -1;
    return (int64_t)(intptr_t)ch;
}

int64_t pal_channel_send(int64_t handle, const char *data) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch) return -1;
    if (ch->count >= CHANNEL_CAP) return -1; /* full */

    size_t len = rt_managed_len(data);
    if (len > CHANNEL_MSG_MAX) len = CHANNEL_MSG_MAX;

    char *buf = (char *)malloc(len + 1);
    if (!buf) return -1;
    memcpy(buf, data, len);
    buf[len] = '\0';

    int idx = (ch->tail) % CHANNEL_CAP;
    ch->msgs[idx].data = buf;
    ch->msgs[idx].len = len;
    ch->tail = (ch->tail + 1) % CHANNEL_CAP;
    ch->count++;
    return 0;
}

char *pal_channel_recv(int64_t handle) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch || ch->count == 0) return rt_strdup_raw("");

    int idx = (ch->head) % CHANNEL_CAP;
    char *result = rt_strdup_raw_n(ch->msgs[idx].data, ch->msgs[idx].len);
    free(ch->msgs[idx].data);
    ch->msgs[idx].data = NULL;
    ch->msgs[idx].len = 0;
    ch->head = (ch->head + 1) % CHANNEL_CAP;
    ch->count--;
    return result;
}

void pal_channel_close(int64_t handle) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch) return;
    for (int i = 0; i < CHANNEL_CAP; i++) {
        if (ch->msgs[i].data) free(ch->msgs[i].data);
    }
    free(ch);
}
