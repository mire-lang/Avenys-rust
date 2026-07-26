// helpers.c — File I/O and byte-access utilities used by kioto.
// Extracted from the former crypto.c to keep crypto builtins out of avenys.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

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
