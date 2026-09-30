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
#include "../pal/pal_crypto.h"

#if PAL_ALLOW_LEGACY_SHELL
extern const char *pal_proc_capture_output(const char *cmd);
#endif

// Computes the RFC 6455 WebSocket accept value without pulling a web
// framework into the compiler runtime. The result is runtime-managed so a
// Mire-only log server can perform a standards-compliant upgrade handshake.
char *rt_websocket_accept(const char *client_key) {
    static const char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    static const char guid[] = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    uint8_t input[256];
    size_t key_len = client_key ? strlen(client_key) : 0;
    size_t guid_len = sizeof(guid) - 1;
    if (key_len + guid_len >= sizeof(input)) return rt_managed_from_cstr("");
    memcpy(input, client_key, key_len);
    memcpy(input + key_len, guid, guid_len);
    size_t length = key_len + guid_len;

    uint32_t h[5] = {0x67452301u, 0xEFCDAB89u, 0x98BADCFEu, 0x10325476u, 0xC3D2E1F0u};
    uint8_t block[128] = {0};
    memcpy(block, input, length);
    block[length] = 0x80;
    size_t padded = length + 1;
    while ((padded % 64) != 56) padded++;
    uint64_t bits = (uint64_t)length * 8u;
    for (int i = 0; i < 8; i++) block[padded + i] = (uint8_t)(bits >> (56 - i * 8));
    padded += 8;
    for (size_t offset = 0; offset < padded; offset += 64) {
        uint32_t w[80];
        for (int i = 0; i < 16; i++) {
            size_t p = offset + (size_t)i * 4;
            w[i] = ((uint32_t)block[p] << 24) | ((uint32_t)block[p + 1] << 16) |
                   ((uint32_t)block[p + 2] << 8) | block[p + 3];
        }
        for (int i = 16; i < 80; i++) {
            uint32_t v = w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16];
            w[i] = (v << 1) | (v >> 31);
        }
        uint32_t a = h[0], b = h[1], c = h[2], d = h[3], e = h[4];
        for (int i = 0; i < 80; i++) {
            uint32_t f, k;
            if (i < 20) { f = (b & c) | ((~b) & d); k = 0x5A827999u; }
            else if (i < 40) { f = b ^ c ^ d; k = 0x6ED9EBA1u; }
            else if (i < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8F1BBCDCu; }
            else { f = b ^ c ^ d; k = 0xCA62C1D6u; }
            uint32_t rotated = (a << 5) | (a >> 27);
            uint32_t temp = rotated + f + e + k + w[i];
            e = d; d = c; c = (b << 30) | (b >> 2); b = a; a = temp;
        }
        h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e;
    }
    uint8_t digest[20];
    for (int i = 0; i < 5; i++) {
        digest[i * 4] = (uint8_t)(h[i] >> 24);
        digest[i * 4 + 1] = (uint8_t)(h[i] >> 16);
        digest[i * 4 + 2] = (uint8_t)(h[i] >> 8);
        digest[i * 4 + 3] = (uint8_t)h[i];
    }
    char encoded[29];
    size_t out = 0;
    for (int i = 0; i < 20; i += 3) {
        uint32_t v = ((uint32_t)digest[i] << 16) |
                     ((uint32_t)(i + 1 < 20 ? digest[i + 1] : 0) << 8) |
                     (uint32_t)(i + 2 < 20 ? digest[i + 2] : 0);
        encoded[out++] = alphabet[(v >> 18) & 63];
        encoded[out++] = alphabet[(v >> 12) & 63];
        encoded[out++] = i + 1 < 20 ? alphabet[(v >> 6) & 63] : '=';
        encoded[out++] = i + 2 < 20 ? alphabet[v & 63] : '=';
    }
    encoded[out] = '\0';
    return rt_managed_from_cstr(encoded);
}

// Builds one unmasked server-to-client text frame. The live dashboard sends
// only the compact global count, so the 125-byte short-frame form is enough.
char *rt_websocket_text_frame(const char *payload) {
    if (!payload) return rt_managed_from_cstr("");
    size_t length = strlen(payload);
    if (length > 125) return rt_managed_from_cstr("");
    uint8_t frame[127];
    frame[0] = 0x81;
    frame[1] = (uint8_t)length;
    memcpy(frame + 2, payload, length);
    return rt_managed_from_slice((const char *)frame, length + 2);
}

/* Appends one already-formatted server event without exposing arbitrary file
 * reads or paths to the HTTP layer. The caller owns the fixed log location. */
int64_t rt_web_log_append(const char *path, const char *line) {
    if (!path || !line) return -1;
    FILE *file = fopen(path, "ab");
    if (!file) return -1;
    size_t length = strlen(line);
    size_t written = fwrite(line, 1, length, file);
    fputc('\n', file);
    fclose(file);
    return written == length ? 0 : -1;
}

// Raw byte access from a managed string.
int64_t rt_crypto_byte_at(const char *s, int64_t i) {
    if (!s || i < 0) return 0;
    return (int64_t)(unsigned char)s[i];
}

/* Hash helpers are runtime-owned so Kioto, Owl and ordinary Mire programs
 * share one vetted libsodium implementation. The returned strings are owned
 * by the Mire runtime and use lowercase hexadecimal encoding. */
static char *rt_crypto_hex(const unsigned char *digest, size_t digest_len) {
    static const char hex[] = "0123456789abcdef";
    char encoded[129];
    if (!digest || digest_len > 64) return rt_managed_from_cstr("");
    for (size_t i = 0; i < digest_len; i++) {
        encoded[i * 2] = hex[digest[i] >> 4];
        encoded[i * 2 + 1] = hex[digest[i] & 0x0f];
    }
    encoded[digest_len * 2] = '\0';
    return rt_managed_from_cstr(encoded);
}

char *rt_crypto_sha256_hex(const char *s) {
    unsigned char digest[PAL_CRYPTO_SHA256_BYTES];
    if (!s || pal_crypto_sha256((const unsigned char *)s, strlen(s), digest) != PAL_ERR_OK)
        return rt_managed_from_cstr("");
    return rt_crypto_hex(digest, sizeof(digest));
}

char *rt_crypto_sha512_hex(const char *s) {
    unsigned char digest[PAL_CRYPTO_SHA512_BYTES];
    if (!s || pal_crypto_sha512((const unsigned char *)s, strlen(s), digest) != PAL_ERR_OK)
        return rt_managed_from_cstr("");
    return rt_crypto_hex(digest, sizeof(digest));
}

// Reads a whole file as raw bytes (binary-safe, embedded NULs preserved).
// On success returns a malloc'd buffer and stores the byte count in *out_len;
// the caller owns the buffer. Returns NULL on any failure.
static unsigned char *rt_read_file_raw(const char *path, size_t *out_len) {
    if (!path) return NULL;
    FILE *file = fopen(path, "rb");
    if (!file) return NULL;
    if (fseek(file, 0, SEEK_END) != 0) { fclose(file); return NULL; }
    long length = ftell(file);
    if (length < 0) { fclose(file); return NULL; }
    if (fseek(file, 0, SEEK_SET) != 0) { fclose(file); return NULL; }
    unsigned char *buf = (unsigned char *)malloc(length ? (size_t)length : 1);
    if (!buf) { fclose(file); return NULL; }
    size_t read = fread(buf, 1, (size_t)length, file);
    fclose(file);
    if (read != (size_t)length) { free(buf); return NULL; }
    *out_len = (size_t)length;
    return buf;
}

char *rt_crypto_sha256_file_hex(const char *path) {
    size_t length = 0;
    unsigned char *data = rt_read_file_raw(path, &length);
    if (!data) return rt_managed_from_cstr("");
    unsigned char digest[PAL_CRYPTO_SHA256_BYTES];
    pal_error_code_t rc = pal_crypto_sha256(data, length, digest);
    free(data);
    if (rc != PAL_ERR_OK) return rt_managed_from_cstr("");
    return rt_crypto_hex(digest, sizeof(digest));
}

char *rt_crypto_sha512_file_hex(const char *path) {
    size_t length = 0;
    unsigned char *data = rt_read_file_raw(path, &length);
    if (!data) return rt_managed_from_cstr("");
    unsigned char digest[PAL_CRYPTO_SHA512_BYTES];
    pal_error_code_t rc = pal_crypto_sha512(data, length, digest);
    free(data);
    if (rc != PAL_ERR_OK) return rt_managed_from_cstr("");
    return rt_crypto_hex(digest, sizeof(digest));
}

// ── Base64 (shared by kioto's crypto bindings) ──────────────────────────────
static const char rt_b64_alphabet[] =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static char *rt_base64_encode(const unsigned char *data, size_t length) {
    if (!data) return rt_managed_from_cstr("");
    size_t out_len = 4 * ((length + 2) / 3);
    char *out = (char *)malloc(out_len + 1);
    if (!out) return rt_managed_from_cstr("");
    size_t i = 0, j = 0;
    while (i + 3 <= length) {
        unsigned int triple = ((unsigned int)data[i] << 16) |
                              ((unsigned int)data[i + 1] << 8) |
                              (unsigned int)data[i + 2];
        out[j++] = rt_b64_alphabet[(triple >> 18) & 0x3F];
        out[j++] = rt_b64_alphabet[(triple >> 12) & 0x3F];
        out[j++] = rt_b64_alphabet[(triple >> 6) & 0x3F];
        out[j++] = rt_b64_alphabet[triple & 0x3F];
        i += 3;
    }
    size_t rem = length - i;
    if (rem == 1) {
        unsigned int triple = (unsigned int)data[i] << 16;
        out[j++] = rt_b64_alphabet[(triple >> 18) & 0x3F];
        out[j++] = rt_b64_alphabet[(triple >> 12) & 0x3F];
        out[j++] = '=';
        out[j++] = '=';
    } else if (rem == 2) {
        unsigned int triple = ((unsigned int)data[i] << 16) |
                              ((unsigned int)data[i + 1] << 8);
        out[j++] = rt_b64_alphabet[(triple >> 18) & 0x3F];
        out[j++] = rt_b64_alphabet[(triple >> 12) & 0x3F];
        out[j++] = rt_b64_alphabet[(triple >> 6) & 0x3F];
        out[j++] = '=';
    }
    out[j] = '\0';
    char *managed = rt_managed_from_cstr(out);
    free(out);
    return managed;
}

static int rt_b64_value(int c) {
    if (c >= 'A' && c <= 'Z') return c - 'A';
    if (c >= 'a' && c <= 'z') return c - 'a' + 26;
    if (c >= '0' && c <= '9') return c - '0' + 52;
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

// Decodes base64 text into a malloc'd byte buffer (caller frees). Whitespace is
// ignored; padding is honoured. Returns NULL on malformed input.
static unsigned char *rt_base64_decode(const char *s, size_t s_len, size_t *out_len) {
    if (!s) return NULL;
    unsigned char *out = (unsigned char *)malloc((s_len / 4 + 2) * 3);
    if (!out) return NULL;
    int accum = 0, bits = 0;
    size_t n = 0;
    for (size_t i = 0; i < s_len; i++) {
        int c = (unsigned char)s[i];
        if (c == '=' || c == '\n' || c == '\r' || c == ' ' || c == '\t') continue;
        int v = rt_b64_value(c);
        if (v < 0) { free(out); return NULL; }
        accum = (accum << 6) | v;
        bits += 6;
        if (bits >= 8) {
            bits -= 8;
            out[n++] = (unsigned char)((accum >> bits) & 0xFF);
        }
    }
    *out_len = n;
    return out;
}

char *rt_crypto_base64_file(const char *path) {
    size_t length = 0;
    unsigned char *data = rt_read_file_raw(path, &length);
    if (!data) return rt_managed_from_cstr("");
    char *out = rt_base64_encode(data, length);
    free(data);
    return out;
}

bool rt_crypto_ed25519_verify_b64(const char *pubkey_b64, const char *data_file,
                                  const char *sig_b64) {
    size_t pk_len = 0, sg_len = 0, data_len = 0;
    unsigned char *pk = rt_base64_decode(pubkey_b64, pubkey_b64 ? strlen(pubkey_b64) : 0, &pk_len);
    unsigned char *sg = rt_base64_decode(sig_b64, sig_b64 ? strlen(sig_b64) : 0, &sg_len);
    unsigned char *data = rt_read_file_raw(data_file, &data_len);
    bool ok = false;
    if (pk && sg && data &&
        pk_len == PAL_CRYPTO_ED25519_PUBLICKEYBYTES &&
        sg_len == PAL_CRYPTO_ED25519_BYTES) {
        ok = pal_crypto_ed25519_verify(data, data_len, sg, sg_len, pk) == PAL_ERR_OK;
    }
    free(pk);
    free(sg);
    free(data);
    return ok;
}

bool rt_crypto_ed25519_verify_file(const char *pubkey_b64, const char *data_file,
                                   const char *sig_file) {
    size_t pk_len = 0, sg_len = 0, data_len = 0;
    unsigned char *pk = rt_base64_decode(pubkey_b64, pubkey_b64 ? strlen(pubkey_b64) : 0, &pk_len);
    unsigned char *sg = rt_read_file_raw(sig_file, &sg_len);
    unsigned char *data = rt_read_file_raw(data_file, &data_len);
    bool ok = false;
    if (pk && sg && data &&
        pk_len == PAL_CRYPTO_ED25519_PUBLICKEYBYTES &&
        sg_len == PAL_CRYPTO_ED25519_BYTES) {
        ok = pal_crypto_ed25519_verify(data, data_len, sg, sg_len, pk) == PAL_ERR_OK;
    }
    free(pk);
    free(sg);
    free(data);
    return ok;
}

// Extracts the raw 32-byte Ed25519 public key from a PEM file and returns it
// base64-encoded. The SubjectPublicKeyInfo DER ends with the raw key, so the
// trailing 32 bytes are used. Returns "" on failure.
char *rt_crypto_ed25519_pubkey_raw_b64(const char *pem_path) {
    size_t pem_len = 0;
    unsigned char *pem = rt_read_file_raw(pem_path, &pem_len);
    if (!pem) return rt_managed_from_cstr("");
    char *body = (char *)malloc(pem_len + 1);
    if (!body) { free(pem); return rt_managed_from_cstr(""); }
    size_t bn = 0;
    size_t i = 0;
    while (i < pem_len) {
        size_t start = i;
        while (i < pem_len && pem[i] != '\n') i++;
        size_t line_len = i - start;
        if (i < pem_len) i++;
        size_t s = start;
        while (s < start + line_len && (pem[s] == ' ' || pem[s] == '\r' || pem[s] == '\t')) s++;
        if (start + line_len - s >= 5 && pem[s] == '-' && pem[s + 1] == '-') continue;
        for (size_t k = s; k < start + line_len; k++) {
            if (pem[k] != '\r' && pem[k] != ' ' && pem[k] != '\t') body[bn++] = (char)pem[k];
        }
    }
    body[bn] = '\0';
    free(pem);
    size_t der_len = 0;
    unsigned char *der = rt_base64_decode(body, bn, &der_len);
    free(body);
    if (!der || der_len < PAL_CRYPTO_ED25519_PUBLICKEYBYTES) {
        free(der);
        return rt_managed_from_cstr("");
    }
    unsigned char *raw = der + (der_len - PAL_CRYPTO_ED25519_PUBLICKEYBYTES);
    char *out = rt_base64_encode(raw, PAL_CRYPTO_ED25519_PUBLICKEYBYTES);
    free(der);
    return out;
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
#if PAL_ALLOW_LEGACY_SHELL
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
// Thread-local exit status of the most recent argv-based capture. Mirrors the
// shell capture's implicit status without needing an extra out-parameter that
// Mire cannot express. Single mire process = single sequential proc user.
static _Thread_local __attribute__((tls_model("global-dynamic"))) int64_t g_last_proc_exit = -1;

char *rt_proc_capture_argv(const char *cmd, void *args_vec) {
    g_last_proc_exit = -1;
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
    int status = 0;
    if (waitpid(pid, &status, 0) > 0) {
        if (WIFEXITED(status)) {
            g_last_proc_exit = WEXITSTATUS(status);
        } else if (WIFSIGNALED(status)) {
            g_last_proc_exit = 128 + WTERMSIG(status);
        }
    }
    char *managed = rt_managed_from_slice(buf, len);
    free(buf);
    rt_free_argv(argv, argc);
    return managed;
}

// Thread-local exit status of the most recent argv-based capture. Mirrors the
// shell capture's implicit status without needing an extra out-parameter that
// Mire cannot express. Single mire process = single sequential proc user.
int64_t rt_proc_last_exit(void) {
    return g_last_proc_exit;
}

// Extended argv capture: optional working directory + optional stderr merge.
// Everything else mirrors rt_proc_capture_argv (pipe + fork + execvp, no shell).
char *rt_proc_capture_argv2(const char *cmd, void *args_vec, const char *cwd, int64_t merge_err) {
    g_last_proc_exit = -1;
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
        close(pipefd[0]);
        if (cwd && *cwd && chdir(cwd) != 0) _exit(126);
        dup2(pipefd[1], STDOUT_FILENO);
        if (merge_err != 0) dup2(pipefd[1], STDERR_FILENO);
        close(pipefd[1]);
        execvp(argv[0], (char *const *)argv);
        _exit(127);
    }

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
    int status = 0;
    if (waitpid(pid, &status, 0) > 0) {
        if (WIFEXITED(status)) {
            g_last_proc_exit = WEXITSTATUS(status);
        } else if (WIFSIGNALED(status)) {
            g_last_proc_exit = 128 + WTERMSIG(status);
        }
    }
    char *managed = rt_managed_from_slice(buf, len);
    free(buf);
    rt_free_argv(argv, argc);
    return managed;
}

// Read one line from the controlling terminal. No shell, no subprocess.
// Non-interactive contexts (no /dev/tty) default to "y" so a Y/n prompt can
// proceed — the same default the shell-based `read ... || echo y` produced.
char *rt_read_tty(void) {
    FILE *tty = fopen("/dev/tty", "r");
    if (!tty) return rt_managed_from_cstr("y");
    char buf[256];
    if (fgets(buf, sizeof(buf), tty) == NULL) {
        fclose(tty);
        return rt_managed_from_cstr("y");
    }
    fclose(tty);
    size_t len = strlen(buf);
    while (len > 0 && (buf[len - 1] == '\n' || buf[len - 1] == '\r')) {
        buf[--len] = '\0';
    }
    return rt_managed_from_slice(buf, len);
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

// ── 5x7 bitmap font (public domain, standard font5x7) ────────────────
// 95 printable ASCII glyphs (code 32..126), each 5 columns × 7 rows.
// Bit 0 of each column byte = top pixel, bit 6 = bottom.
// Data adapted from the standard font5x7 public-domain release.
static const uint8_t s_font5x7[95][5] = {
    {0x00,0x00,0x00,0x00,0x00}, // 0x20 space
    {0x00,0x00,0x5F,0x00,0x00}, // 0x21 !
    {0x00,0x07,0x00,0x07,0x00}, // 0x22 "
    {0x14,0x7F,0x14,0x7F,0x14}, // 0x23 #
    {0x24,0x2A,0x71,0x2A,0x12}, // 0x24 $
    {0x23,0x13,0x08,0x64,0x62}, // 0x25 %
    {0x36,0x49,0x55,0x22,0x50}, // 0x26 &
    {0x00,0x05,0x03,0x00,0x00}, // 0x27 '
    {0x00,0x1C,0x22,0x41,0x00}, // 0x28 (
    {0x00,0x41,0x22,0x1C,0x00}, // 0x29 )
    {0x08,0x2A,0x1C,0x2A,0x08}, // 0x2A *
    {0x08,0x08,0x3E,0x08,0x08}, // 0x2B +
    {0x00,0x00,0x50,0x00,0x00}, // 0x2C ,
    {0x08,0x08,0x08,0x08,0x08}, // 0x2D -
    {0x00,0x00,0x60,0x00,0x00}, // 0x2E .
    {0x20,0x10,0x08,0x04,0x02}, // 0x2F /
    {0x3E,0x51,0x49,0x45,0x3E}, // 0x30 0
    {0x00,0x42,0x7F,0x40,0x00}, // 0x31 1
    {0x62,0x93,0x91,0x91,0x4E}, // 0x32 2
    {0x22,0x81,0x91,0x91,0x7E}, // 0x33 3
    {0x18,0x28,0x24,0x3F,0x20}, // 0x34 4
    {0x7F,0x89,0x89,0x89,0x06}, // 0x35 5
    {0x7E,0x89,0x89,0x89,0x76}, // 0x36 6
    {0x01,0x71,0x09,0x05,0x03}, // 0x37 7
    {0x36,0x49,0x49,0x49,0x36}, // 0x38 8
    {0x06,0x49,0x49,0x29,0x1E}, // 0x39 9
    {0x00,0x00,0x24,0x00,0x00}, // 0x3A :
    {0x00,0x50,0x30,0x00,0x00}, // 0x3B ;
    {0x00,0x08,0x14,0x22,0x41}, // 0x3C <
    {0x00,0x14,0x14,0x14,0x14}, // 0x3D =
    {0x41,0x22,0x14,0x08,0x00}, // 0x3E >
    {0x02,0x01,0x51,0x09,0x06}, // 0x3F ?
    {0x3E,0x41,0x5D,0x59,0x1E}, // 0x40 @
    {0x7E,0x09,0x09,0x09,0x7E}, // 0x41 A
    {0x7F,0x49,0x49,0x49,0x3E}, // 0x42 B
    {0x7E,0x41,0x41,0x41,0x22}, // 0x43 C
    {0x7F,0x41,0x41,0x41,0x3E}, // 0x44 D
    {0x7F,0x49,0x49,0x49,0x41}, // 0x45 E
    {0x7F,0x09,0x09,0x09,0x01}, // 0x46 F
    {0x7E,0x41,0x41,0x51,0x72}, // 0x47 G
    {0x7F,0x08,0x08,0x08,0x7F}, // 0x48 H
    {0x41,0x41,0x7F,0x41,0x41}, // 0x49 I
    {0x07,0x04,0x04,0x04,0x7C}, // 0x4A J
    {0x7F,0x08,0x14,0x22,0x41}, // 0x4B K
    {0x7F,0x40,0x40,0x40,0x40}, // 0x4C L
    {0x7F,0x02,0x0C,0x02,0x7F}, // 0x4D M
    {0x7F,0x04,0x08,0x10,0x7F}, // 0x4E N
    {0x7E,0x41,0x41,0x41,0x7E}, // 0x4F O
    {0x7F,0x09,0x09,0x09,0x06}, // 0x50 P
    {0x7E,0x41,0x51,0x21,0x5E}, // 0x51 Q
    {0x7F,0x09,0x19,0x29,0x46}, // 0x52 R
    {0x46,0x49,0x49,0x49,0x31}, // 0x53 S
    {0x01,0x01,0x7F,0x01,0x01}, // 0x54 T
    {0x3F,0x40,0x40,0x40,0x3F}, // 0x55 U
    {0x1F,0x20,0x40,0x20,0x1F}, // 0x56 V
    {0x7F,0x20,0x18,0x20,0x7F}, // 0x57 W
    {0x63,0x14,0x08,0x14,0x63}, // 0x58 X
    {0x03,0x04,0x78,0x04,0x03}, // 0x59 Y
    {0x61,0x51,0x49,0x45,0x43}, // 0x5A Z
    {0x00,0x7F,0x41,0x41,0x00}, // 0x5B [
    {0x02,0x04,0x08,0x10,0x20}, // 0x5C \\
    {0x00,0x41,0x41,0x7F,0x00}, // 0x5D ]
    {0x04,0x02,0x01,0x02,0x04}, // 0x5E ^
    {0x40,0x40,0x40,0x40,0x40}, // 0x5F _
    {0x00,0x01,0x02,0x04,0x00}, // 0x60 `
    {0x20,0x54,0x54,0x54,0x78}, // 0x61 a
    {0x7F,0x48,0x44,0x44,0x38}, // 0x62 b
    {0x38,0x44,0x44,0x44,0x20}, // 0x63 c
    {0x38,0x44,0x44,0x48,0x7F}, // 0x64 d
    {0x38,0x54,0x54,0x54,0x18}, // 0x65 e
    {0x01,0x7F,0x09,0x09,0x06}, // 0x66 f
    {0x00,0x38,0x44,0x44,0x38}, // 0x67 g
    {0x7F,0x08,0x04,0x04,0x38}, // 0x68 h
    {0x00,0x00,0x4F,0x00,0x00}, // 0x69 i
    {0x20,0x40,0x40,0x40,0x4F}, // 0x6A j
    {0x00,0x7F,0x10,0x28,0x44}, // 0x6B k
    {0x00,0x41,0x40,0x40,0x40}, // 0x6C l
    {0x7C,0x04,0x38,0x04,0x78}, // 0x6D m
    {0x7C,0x08,0x04,0x04,0x78}, // 0x6E n
    {0x38,0x44,0x44,0x44,0x38}, // 0x6F o
    {0x7C,0x14,0x14,0x14,0x08}, // 0x70 p
    {0x08,0x14,0x14,0x14,0x7C}, // 0x71 q
    {0x7C,0x08,0x04,0x04,0x08}, // 0x72 r
    {0x24,0x4A,0x4A,0x4A,0x12}, // 0x73 s
    {0x04,0x04,0x7F,0x04,0x04}, // 0x74 t
    {0x3C,0x40,0x40,0x40,0x3C}, // 0x75 u
    {0x1C,0x20,0x40,0x20,0x1C}, // 0x76 v
    {0x3C,0x40,0x70,0x40,0x3C}, // 0x77 w
    {0x44,0x28,0x10,0x28,0x44}, // 0x78 x
    {0x0C,0x50,0x50,0x50,0x3C}, // 0x79 y
    {0x44,0x64,0x54,0x4C,0x44}, // 0x7A z
    {0x00,0x08,0x14,0x22,0x41}, // 0x7B {
    {0x00,0x00,0xFF,0x00,0x00}, // 0x7C |
    {0x41,0x22,0x14,0x08,0x00}, // 0x7D }
    {0x08,0x04,0x08,0x10,0x08}, // 0x7E ~
};

// ── UTF-8 decoding for the bitmap font ──────────────────────────────
// Decode the codepoint whose UTF-8 sequence starts at byte index `i` in
// `s`. Returns the codepoint and writes the sequence length (1..4) to
// *seq_len. Invalid/truncated sequences degrade to the raw byte.
// Assumes `s` is a valid NUL-terminated string (Mire managed strings are).
static int64_t utf8_decode_at(const char *s, int64_t i, int64_t *seq_len) {
    const unsigned char *u = (const unsigned char *)s;
    unsigned char b = u[i];
    if (b < 0x80) { *seq_len = 1; return b; }
    if (u[i+1] && (b & 0xE0) == 0xC0 && (u[i+1] & 0xC0) == 0x80) {
        *seq_len = 2;
        return ((int64_t)(b & 0x1F) << 6) | (u[i+1] & 0x3F);
    }
    if (u[i+1] && u[i+2] && (b & 0xF0) == 0xE0 && (u[i+1] & 0xC0) == 0x80 && (u[i+2] & 0xC0) == 0x80) {
        *seq_len = 3;
        return ((int64_t)(b & 0x0F) << 12) | ((int64_t)(u[i+1] & 0x3F) << 6) | (u[i+2] & 0x3F);
    }
    if (u[i+1] && u[i+2] && u[i+3] && (b & 0xF8) == 0xF0 && (u[i+1] & 0xC0) == 0x80 && (u[i+2] & 0xC0) == 0x80 && (u[i+3] & 0xC0) == 0x80) {
        *seq_len = 4;
        return ((int64_t)(b & 0x07) << 18) | ((int64_t)(u[i+1] & 0x3F) << 12) |
               ((int64_t)(u[i+2] & 0x3F) << 6) | (u[i+3] & 0x3F);
    }
    *seq_len = 1;
    return b;
}

// Codepoint at UTF-8 byte offset i (for Mire draw loops).
int64_t rt_font_char_at(const char *s, int64_t i) {
    if (!s || i < 0) return '?';
    int64_t len = 0;
    return utf8_decode_at(s, i, &len);
}

// Byte-length of the UTF-8 sequence starting at byte offset i (1..4).
int64_t rt_font_char_len(const char *s, int64_t i) {
    if (!s || i < 0) return 1;
    int64_t len = 0;
    utf8_decode_at(s, i, &len);
    return len;
}

// ── Latin-1 (Spanish) glyph support ─────────────────────────────────
// Accented letters are the base glyph with a small accent mark drawn on
// the free top rows; special punctuation (¡ ¿) has dedicated glyphs.
static int latin1_accent_pixel(int accent, int col, int row) {
    switch (accent) {
        case 1: // acute ´
            return (col == 2 && row == 0) || (col == 3 && row == 1);
        case 2: // grave `
            return (col == 2 && row == 0) || (col == 1 && row == 1);
        case 3: // circumflex ^
            return (col == 1 && row == 0) || (col == 3 && row == 0) || (col == 2 && row == 1);
        case 4: // tilde ~
            return (col >= 1 && col <= 3 && row == 0);
        case 5: // diaeresis ¨
            return (col == 1 && row == 0) || (col == 3 && row == 0);
        case 6: // cedilla ¸ (hook under the letter)
            return ((col == 0 || col == 1) && row == 6);
        default:
            return 0;
    }
}

// Map a Latin-1 codepoint to its base ASCII letter + accent id (1..6).
// Returns 0 for codepoints without a base-letter decomposition.
static int latin1_base_accent(int64_t cp, int *accent) {
    switch (cp) {
        case 0xC0: *accent = 2; return 'A'; case 0xC1: *accent = 1; return 'A';
        case 0xC2: *accent = 3; return 'A'; case 0xC3: *accent = 4; return 'A';
        case 0xC4: *accent = 5; return 'A'; case 0xC5: *accent = 0; return 'A';
        case 0xC7: *accent = 6; return 'C';
        case 0xC8: *accent = 2; return 'E'; case 0xC9: *accent = 1; return 'E';
        case 0xCA: *accent = 3; return 'E'; case 0xCB: *accent = 5; return 'E';
        case 0xCC: *accent = 2; return 'I'; case 0xCD: *accent = 1; return 'I';
        case 0xCE: *accent = 3; return 'I'; case 0xCF: *accent = 5; return 'I';
        case 0xD1: *accent = 4; return 'N';
        case 0xD2: *accent = 2; return 'O'; case 0xD3: *accent = 1; return 'O';
        case 0xD4: *accent = 3; return 'O'; case 0xD5: *accent = 4; return 'O';
        case 0xD6: *accent = 5; return 'O';
        case 0xD9: *accent = 2; return 'U'; case 0xDA: *accent = 1; return 'U';
        case 0xDB: *accent = 3; return 'U'; case 0xDC: *accent = 5; return 'U';
        case 0xDD: *accent = 1; return 'Y';
        case 0xE0: *accent = 2; return 'a'; case 0xE1: *accent = 1; return 'a';
        case 0xE2: *accent = 3; return 'a'; case 0xE3: *accent = 4; return 'a';
        case 0xE4: *accent = 5; return 'a'; case 0xE5: *accent = 0; return 'a';
        case 0xE7: *accent = 6; return 'c';
        case 0xE8: *accent = 2; return 'e'; case 0xE9: *accent = 1; return 'e';
        case 0xEA: *accent = 3; return 'e'; case 0xEB: *accent = 5; return 'e';
        case 0xEC: *accent = 2; return 'i'; case 0xED: *accent = 1; return 'i';
        case 0xEE: *accent = 3; return 'i'; case 0xEF: *accent = 5; return 'i';
        case 0xF1: *accent = 4; return 'n';
        case 0xF2: *accent = 2; return 'o'; case 0xF3: *accent = 1; return 'o';
        case 0xF4: *accent = 3; return 'o'; case 0xF5: *accent = 4; return 'o';
        case 0xF6: *accent = 5; return 'o';
        case 0xF8: *accent = 0; return 'o';
        case 0xF9: *accent = 2; return 'u'; case 0xFA: *accent = 1; return 'u';
        case 0xFB: *accent = 3; return 'u'; case 0xFC: *accent = 5; return 'u';
        case 0xFD: *accent = 1; return 'y'; case 0xFF: *accent = 5; return 'y';
        default: return 0;
    }
}

// Special punctuation glyphs: ¡ (0xA1) and ¿ (0xBF).
static int latin1_special_pixel(int64_t cp, int col, int row) {
    if (col < 0 || col >= 5 || row < 0 || row >= 7) return 0;
    if (cp == 0xA1) { // ¡ inverted exclamation: dot on top, bar below
        if (col == 2) {
            if (row == 0) return 1;
            if (row >= 3 && row <= 6) return 1;
        }
        return 0;
    }
    if (cp == 0xBF) { // ¿ inverted question mark
        static const uint8_t inv_q[5] = {0x04,0x48,0x48,0x28,0x10};
        return (inv_q[col] >> row) & 1;
    }
    return 0;
}

int64_t rt_font_get_pixel(int64_t ch, int64_t col, int64_t row) {
    if (col < 0 || col >= 5 || row < 0 || row >= 7) return 0;
    int64_t cp = ch;
    if (cp >= 32 && cp <= 126) {
        return (s_font5x7[cp - 32][col] >> row) & 1;
    }
    if (cp >= 0xA1 && cp <= 0xFF) {
        if (latin1_special_pixel(cp, col, row)) return 1;
        int accent = 0;
        int base = latin1_base_accent(cp, &accent);
        if (base) {
            if (accent && latin1_accent_pixel(accent, col, row)) return 1;
            return (s_font5x7[base - 32][col] >> row) & 1;
        }
    }
    return (s_font5x7['?' - 32][col] >> row) & 1;
}
