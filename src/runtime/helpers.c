// helpers.c — File I/O and byte-access utilities used by kioto.
// Extracted from the former crypto.c to keep crypto builtins out of avenys.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include "runtime.h"
#include "pal.h"

#ifdef PAL_ALLOW_LEGACY_SHELL
extern const char *pal_proc_capture_output(const char *cmd);
#endif

// Raw byte access from a managed string.
int64_t rt_crypto_byte_at(const char *s, int64_t i) {
    if (!s || i < 0) return 0;
    return (int64_t)(unsigned char)s[i];
}

// Read an entire file as a managed string (binary-safe).
char *rt_read_bytes(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (len <= 0) { fclose(f); return NULL; }
    char *buf = (char *)malloc((size_t)len + 1);
    if (!buf) { fclose(f); return NULL; }
    size_t rd = fread(buf, 1, (size_t)len, f);
    fclose(f);
    buf[rd] = '\0';
    return buf;
}

// Read an entire file and return it as a runtime-managed string.
// Does NOT use pal_fs_read_file (unsandboxed; and its returned pointer
// ownership is easy to get wrong across FFI). Reads through rt_read_bytes
// and copies into managed storage so the result is always runtime-owned.
char *rt_fs_read_bytes(const char *path) {
    if (!path) return rt_managed_from_cstr("");
    char *raw = rt_read_bytes(path);
    if (!raw) return rt_managed_from_cstr("");
    char *managed = rt_managed_from_cstr(raw);
    free(raw);
    return managed;
}

// Decode a hex string and write the raw bytes to a file.
int rt_hex_to_file(const char *path, const char *hex) {
    if (!path || !hex) return 0;
    size_t hex_len = strlen(hex);
    size_t bin_len = hex_len / 2;
    char *bin = (char *)malloc(bin_len);
    if (!bin) return 0;
    for (size_t i = 0; i < bin_len; i++) {
        unsigned int byte;
        sscanf(hex + 2 * i, "%2x", &byte);
        bin[i] = (char)byte;
    }
    FILE *f = fopen(path, "wb");
    if (!f) { free(bin); return 0; }
    fwrite(bin, 1, bin_len, f);
    fclose(f);
    free(bin);
    return 1;
}

// Runs a command via PAL capture and returns the output as a managed string.
// Requires PAL_ALLOW_LEGACY_SHELL (see pal.h).
#ifdef PAL_ALLOW_LEGACY_SHELL
char *rt_proc_capture_output(const char *cmd) {
    if (!cmd) return rt_managed_from_cstr("");
    const char *out = pal_proc_capture_output(cmd);
    if (!out) return rt_managed_from_cstr("");
    char *managed = rt_managed_from_cstr(out);
    free((void *)out);
    return managed;
}
#endif

// Safe argv-based process execution with output capture.
// Builds a real argv[] (no shell) and runs fork + execvp directly.
// Returns the captured stdout as a managed string (empty on failure).
char *rt_proc_capture_argv(const char *cmd, void *args_vec) {
    if (!cmd || !args_vec) return rt_managed_from_cstr("");
    int64_t argc = 0;
    char **argv = rt_build_argv(cmd, args_vec, &argc);
    if (!argv) return rt_managed_from_cstr("");

    int pipefd[2];
    if (pipe(pipefd) != 0) {
        rt_free_argv(argv, argc);
        return rt_managed_from_cstr("");
    }

    pid_t pid = fork();
    if (pid < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        rt_free_argv(argv, argc);
        return rt_managed_from_cstr("");
    }

    if (pid == 0) {
        // Child: wire stdout to the pipe, exec without a shell.
        close(pipefd[0]);
        dup2(pipefd[1], STDOUT_FILENO);
        close(pipefd[1]);
        execvp(argv[0], (char *const *)argv);
        _exit(127);
    }

    // Parent: read captured output.
    close(pipefd[1]);
    size_t cap = 4096;
    size_t len = 0;
    char *buf = malloc(cap);
    if (!buf) {
        close(pipefd[0]);
        waitpid(pid, NULL, 0);
        rt_free_argv(argv, argc);
        return rt_managed_from_cstr("");
    }
    ssize_t n;
    while ((n = read(pipefd[0], buf + len, cap - len - 1)) > 0) {
        len += (size_t)n;
        if (len + 1 >= cap) {
            cap *= 2;
            char *nb = realloc(buf, cap);
            if (!nb) {
                free(buf);
                close(pipefd[0]);
                waitpid(pid, NULL, 0);
                rt_free_argv(argv, argc);
                return rt_managed_from_cstr("");
            }
            buf = nb;
        }
    }
    close(pipefd[0]);
    buf[len] = '\0';
    waitpid(pid, NULL, 0);
    char *managed = rt_managed_from_slice(buf, len);
    free(buf);
    rt_free_argv(argv, argc);
    return managed;
}

// Safe channel receive into a caller-owned buffer.
// Bridges the PAL pal_bytes_t return (heap-allocated) into a fixed
// caller buffer, releasing the PAL allocation. Returns bytes copied.
int64_t rt_channel_recv_into(int64_t ch_handle, char *buf, int64_t capacity) {
    if (!buf || capacity <= 0) return 0;
    pal_channel_t ch = { (uint32_t)ch_handle, (uint32_t)((uint64_t)ch_handle >> 32) };
    pal_bytes_t out = pal_channel_recv(ch);
    if (!out.data || out.len <= 0) return 0;
    int64_t n = out.len < capacity ? out.len : capacity;
    if (n > 0) memcpy(buf, out.data, (size_t)n);
    pal_free(out.data);
    return n;
}
