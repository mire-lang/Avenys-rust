#include "../pal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>
#include <dirent.h>

#define EXPAND_TILDE(var) char *var##_exp = expand_tilde(var); \
    const char *var##_real = var##_exp ? var##_exp : var
#define EXPAND_TILDE_END(var) free(var##_exp)

static char *expand_tilde(const char *path) {
    if (path && path[0] == '~' && path[1] == '/') {
        const char *home = getenv("HOME");
        if (home) {
            size_t hlen = strlen(home);
            size_t plen = strlen(path) - 1;
            char *out = (char *)malloc(hlen + plen + 1);
            if (out) {
                memcpy(out, home, hlen);
                memcpy(out + hlen, path + 1, plen + 1);
            }
            return out;
        }
    }
    return NULL;
}

int pal_fs_write(const char *path, const char *content) {
    EXPAND_TILDE(path);
    FILE *fh = fopen(path_real, "w");
    int ok = fh ? (fputs(content, fh) >= 0) : 0;
    if (fh) fclose(fh);
    EXPAND_TILDE_END(path);
    return ok ? 1 : 0;
}

int pal_fs_append(const char *path, const char *content) {
    EXPAND_TILDE(path);
    FILE *fh = fopen(path_real, "a");
    int ok = fh ? (fputs(content, fh) >= 0) : 0;
    if (fh) fclose(fh);
    EXPAND_TILDE_END(path);
    return ok ? 1 : 0;
}

char *pal_fs_read(const char *path) {
    EXPAND_TILDE(path);
    FILE *fh = fopen(path_real, "rb");
    if (!fh) { EXPAND_TILDE_END(path); return NULL; }
    fseek(fh, 0, SEEK_END);
    long size = ftell(fh);
    fseek(fh, 0, SEEK_SET);
    char *buf = (char *)malloc((size_t)(size + 1));
    if (!buf) { fclose(fh); EXPAND_TILDE_END(path); return NULL; }
    size_t read = fread(buf, 1, (size_t)size, fh);
    buf[read] = '\0';
    fclose(fh);
    EXPAND_TILDE_END(path);
    return buf;
}

int pal_fs_copy(const char *src, const char *dst) {
    EXPAND_TILDE(src);
    EXPAND_TILDE(dst);
    FILE *in = fopen(src_real, "rb");
    if (!in) { EXPAND_TILDE_END(dst); EXPAND_TILDE_END(src); return 0; }
    FILE *out = fopen(dst_real, "wb");
    if (!out) { fclose(in); EXPAND_TILDE_END(dst); EXPAND_TILDE_END(src); return 0; }
    char buf[4096];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), in)) > 0)
        fwrite(buf, 1, n, out);
    fclose(in);
    fclose(out);
    EXPAND_TILDE_END(dst);
    EXPAND_TILDE_END(src);
    return 1;
}

int pal_fs_move(const char *src, const char *dst) {
    EXPAND_TILDE(src);
    EXPAND_TILDE(dst);
    int r = rename(src_real, dst_real);
    if (r == 0) { EXPAND_TILDE_END(dst); EXPAND_TILDE_END(src); return 1; }
    FILE *in = fopen(src_real, "rb");
    if (!in) { EXPAND_TILDE_END(dst); EXPAND_TILDE_END(src); return 0; }
    FILE *out = fopen(dst_real, "wb");
    if (!out) { fclose(in); EXPAND_TILDE_END(dst); EXPAND_TILDE_END(src); return 0; }
    char buf[4096];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), in)) > 0)
        fwrite(buf, 1, n, out);
    fclose(in);
    fclose(out);
    remove(src_real);
    EXPAND_TILDE_END(dst);
    EXPAND_TILDE_END(src);
    return 1;
}

int pal_fs_delete(const char *path) {
    EXPAND_TILDE(path);
    int result = remove(path_real) == 0 ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int pal_fs_mkdir(const char *path) {
    EXPAND_TILDE(path);
    int result = mkdir(path_real, 0755) == 0 ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int pal_fs_rmdir(const char *path) {
    EXPAND_TILDE(path);
    int result = rmdir(path_real) == 0 ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int64_t pal_fs_exists(const char *path) {
    EXPAND_TILDE(path);
    int64_t result = access(path_real, F_OK) == 0 ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int64_t pal_fs_is_dir(const char *path) {
    EXPAND_TILDE(path);
    struct stat st;
    int64_t result = (stat(path_real, &st) == 0 && S_ISDIR(st.st_mode)) ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int64_t pal_fs_is_file(const char *path) {
    EXPAND_TILDE(path);
    struct stat st;
    int64_t result = (stat(path_real, &st) == 0 && S_ISREG(st.st_mode)) ? 1 : 0;
    EXPAND_TILDE_END(path);
    return result;
}

int64_t pal_fs_size(const char *path) {
    EXPAND_TILDE(path);
    struct stat st;
    int64_t result = 0;
    if (stat(path_real, &st) == 0) result = (int64_t)st.st_size;
    EXPAND_TILDE_END(path);
    return result;
}

void *pal_fs_list(const char *path) {
    extern void *rt_list_create(int64_t initial_cap, int64_t elem_size);
    extern int64_t rt_list_len(void *list_ptr);
    extern void *rt_list_push_ptr(void *list_ptr, void *value);
    extern char *rt_strdup_raw(const char *src);
    EXPAND_TILDE(path);
    void *list = rt_list_create(16, 8);
    DIR *dir = opendir(path_real);
    if (!dir) { EXPAND_TILDE_END(path); return list; }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (entry->d_name[0] == '.' && (entry->d_name[1] == '\0' || (entry->d_name[1] == '.' && entry->d_name[2] == '\0')))
            continue;
        char *name = rt_strdup_raw(entry->d_name);
        list = rt_list_push_ptr(list, name);
    }
    closedir(dir);
    EXPAND_TILDE_END(path);
    return list;
}

char *pal_fs_join(const char *a, const char *b) {
    size_t alen = strlen(a);
    size_t blen = strlen(b);
    char *out = (char *)malloc(alen + blen + 2);
    if (!out) return NULL;
    snprintf(out, alen + blen + 2, "%s/%s", a, b);
    return out;
}

char *pal_fs_dir(const char *path) {
    EXPAND_TILDE(path);
    const char *slash = strrchr(path_real, '/');
    if (!slash) { EXPAND_TILDE_END(path); return NULL; }
    size_t len = (size_t)(slash - path_real);
    char *out = (char *)malloc(len + 1);
    if (!out) { EXPAND_TILDE_END(path); return NULL; }
    memcpy(out, path_real, len);
    out[len] = '\0';
    EXPAND_TILDE_END(path);
    return out;
}

char *pal_fs_name(const char *path) {
    EXPAND_TILDE(path);
    const char *slash = strrchr(path_real, '/');
    const char *base = slash ? slash + 1 : path_real;
    char *out = strdup(base);
    EXPAND_TILDE_END(path);
    return out;
}

char *pal_fs_ext(const char *path) {
    EXPAND_TILDE(path);
    const char *dot = strrchr(path_real, '.');
    if (!dot || dot == path_real) { EXPAND_TILDE_END(path); return NULL; }
    char *out = strdup(dot);
    EXPAND_TILDE_END(path);
    return out;
}
