#include "../pal.h"
#include "../../runtime/runtime.h"
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>

typedef struct {
    char *data;
    size_t len;
    int has_data;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} Channel;

static Channel *channel_new(void) {
    Channel *ch = (Channel *)calloc(1, sizeof(Channel));
    if (!ch) return NULL;
    pthread_mutex_init(&ch->mutex, NULL);
    pthread_cond_init(&ch->cond, NULL);
    return ch;
}

int64_t pal_channel_create(void) {
    Channel *ch = channel_new();
    return (int64_t)(intptr_t)ch;
}

int64_t pal_channel_send(int64_t handle, const char *data) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch) return -1;
    pthread_mutex_lock(&ch->mutex);
    if (ch->data) { free(ch->data); ch->data = NULL; ch->len = 0; }
    size_t len = rt_managed_len(data);
    ch->data = (char *)malloc(len + 1);
    if (!ch->data) { pthread_mutex_unlock(&ch->mutex); return -1; }
    memcpy(ch->data, data, len);
    ch->data[len] = '\0';
    ch->len = len;
    ch->has_data = 1;
    pthread_cond_signal(&ch->cond);
    pthread_mutex_unlock(&ch->mutex);
    return 0;
}

char *pal_channel_recv(int64_t handle) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch) return rt_strdup_raw("");
    pthread_mutex_lock(&ch->mutex);
    while (!ch->has_data) {
        pthread_cond_wait(&ch->cond, &ch->mutex);
    }
    char *result = rt_strdup_raw_n(ch->data, ch->len);
    free(ch->data);
    ch->data = NULL;
    ch->len = 0;
    ch->has_data = 0;
    pthread_mutex_unlock(&ch->mutex);
    return result;
}

void pal_channel_close(int64_t handle) {
    Channel *ch = (Channel *)(intptr_t)handle;
    if (!ch) return;
    pthread_mutex_lock(&ch->mutex);
    if (ch->data) free(ch->data);
    ch->data = NULL;
    pthread_mutex_unlock(&ch->mutex);
    pthread_mutex_destroy(&ch->mutex);
    pthread_cond_destroy(&ch->cond);
    free(ch);
}
