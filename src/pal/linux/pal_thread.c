#include "pal.h"
#include <pthread.h>
#include <stdlib.h>
#include <unistd.h>

int64_t pal_thread_spawn(void *(*fn)(void*), void *arg) {
    pthread_t thread;
    int ret = pthread_create(&thread, NULL, fn, arg);
    if (ret != 0) {
        return -1;
    }
    return (int64_t)thread;
}

int64_t pal_thread_join(int64_t tid, void **result) {
    int ret = pthread_join((pthread_t)tid, result);
    return ret == 0 ? 0 : -1;
}

void pal_thread_exit(void *result) {
    pthread_exit(result);
}

int64_t pal_thread_self(void) {
    return (int64_t)pthread_self();
}

void pal_thread_detach(int64_t tid) {
    pthread_detach((pthread_t)tid);
}
