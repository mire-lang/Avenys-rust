// WASI PAL — Environment
// WASI provides getenv/setenv/getcwd/chdir via wasi-libc.

#include "../pal.h"
#include <stdlib.h>
#include <string.h>

#ifdef __wasi__
#include <unistd.h>
#endif

char *pal_env_get(const char *name) {
    const char *val = getenv(name);
    if (!val) return NULL;
    extern char *rt_strdup_raw(const char *src);
    return rt_strdup_raw(val);
}

int pal_env_set(const char *name, const char *value) {
#ifdef __wasi__
    return setenv(name, value, 1) == 0 ? 1 : 0;
#else
    (void)name; (void)value;
    return 0;
#endif
}

void *pal_env_all(void) {
    extern void *rt_list_create(int64_t initial_cap, int64_t elem_size);
    extern void *rt_list_push_ptr(void *list_ptr, void *value);
    extern char *rt_strdup_raw(const char *src);
    void *list = rt_list_create(32, 8);
#ifdef __wasi__
    extern char **environ;
    for (char **env = environ; *env; env++) {
        char *copy = rt_strdup_raw(*env);
        list = rt_list_push_ptr(list, copy);
    }
#endif
    return list;
}

char *pal_env_cwd(void) {
#ifdef __wasi__
    long size = 4096;
    char *buf = (char *)malloc((size_t)size);
    if (!buf) return NULL;
    if (!getcwd(buf, (size_t)size)) {
        free(buf);
        return NULL;
    }
    return buf;
#else
    char *dot = malloc(2);
    if (!dot) return NULL;
    dot[0] = '.';
    dot[1] = '\0';
    return dot;
#endif
}

int pal_env_chdir(const char *path) {
#ifdef __wasi__
    return chdir(path) == 0 ? 1 : 0;
#else
    (void)path;
    return 0;
#endif
}
