#include "runtime.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ═══════════════════════════════════════════════════════════════════════
//  SHA-256 — pure C implementation (no external dependencies)
//  Used for hashing, integrity checks, and HMAC base.
// ═══════════════════════════════════════════════════════════════════════

static const uint32_t SHA256_K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
};

static const uint32_t SHA256_H0[8] = {
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
};

#define ROR32(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
#define CH(x, y, z) (((x) & (y)) ^ (~(x) & (z)))
#define MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))
#define EP0(x) (ROR32(x, 2) ^ ROR32(x, 13) ^ ROR32(x, 22))
#define EP1(x) (ROR32(x, 6) ^ ROR32(x, 11) ^ ROR32(x, 25))
#define SIG0(x) (ROR32(x, 7) ^ ROR32(x, 18) ^ ((x) >> 3))
#define SIG1(x) (ROR32(x, 17) ^ ROR32(x, 19) ^ ((x) >> 10))

typedef struct {
    uint32_t h[8];
    uint64_t total_len;
    uint8_t  buf[64];
    size_t   buf_len;
} SHA256Ctx;

static void sha256_transform(SHA256Ctx *ctx, const uint8_t block[64]) {
    uint32_t w[64];
    for (int i = 0; i < 16; i++) {
        w[i] = ((uint32_t)block[i*4] << 24) | ((uint32_t)block[i*4+1] << 16)
              | ((uint32_t)block[i*4+2] << 8)  | (uint32_t)block[i*4+3];
    }
    for (int i = 16; i < 64; i++) {
        w[i] = SIG1(w[i-2]) + w[i-7] + SIG0(w[i-15]) + w[i-16];
    }

    uint32_t a = ctx->h[0], b = ctx->h[1], c = ctx->h[2], d = ctx->h[3];
    uint32_t e = ctx->h[4], f = ctx->h[5], g = ctx->h[6], h = ctx->h[7];

    for (int i = 0; i < 64; i++) {
        uint32_t t1 = h + EP1(e) + CH(e, f, g) + SHA256_K[i] + w[i];
        uint32_t t2 = EP0(a) + MAJ(a, b, c);
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }

    ctx->h[0] += a; ctx->h[1] += b; ctx->h[2] += c; ctx->h[3] += d;
    ctx->h[4] += e; ctx->h[5] += f; ctx->h[6] += g; ctx->h[7] += h;
}

static void sha256_init(SHA256Ctx *ctx) {
    memcpy(ctx->h, SHA256_H0, sizeof(SHA256_H0));
    ctx->total_len = 0;
    ctx->buf_len = 0;
}

static void sha256_update(SHA256Ctx *ctx, const uint8_t *data, size_t len) {
    ctx->total_len += len;
    size_t off = 0;
    if (ctx->buf_len > 0) {
        size_t fill = 64 - ctx->buf_len;
        if (len < fill) { memcpy(ctx->buf + ctx->buf_len, data, len); ctx->buf_len += len; return; }
        memcpy(ctx->buf + ctx->buf_len, data, fill);
        sha256_transform(ctx, ctx->buf);
        off = fill;
        ctx->buf_len = 0;
    }
    while (off + 64 <= len) {
        sha256_transform(ctx, data + off);
        off += 64;
    }
    if (off < len) {
        memcpy(ctx->buf, data + off, len - off);
        ctx->buf_len = len - off;
    }
}

static void sha256_final(SHA256Ctx *ctx, uint8_t out[32]) {
    uint64_t total_bits = ctx->total_len * 8;
    ctx->buf[ctx->buf_len++] = 0x80;
    if (ctx->buf_len > 56) {
        memset(ctx->buf + ctx->buf_len, 0, 64 - ctx->buf_len);
        sha256_transform(ctx, ctx->buf);
        ctx->buf_len = 0;
    }
    memset(ctx->buf + ctx->buf_len, 0, 56 - ctx->buf_len);
    ctx->buf[56] = (uint8_t)(total_bits >> 56); ctx->buf[57] = (uint8_t)(total_bits >> 48);
    ctx->buf[58] = (uint8_t)(total_bits >> 40); ctx->buf[59] = (uint8_t)(total_bits >> 32);
    ctx->buf[60] = (uint8_t)(total_bits >> 24); ctx->buf[61] = (uint8_t)(total_bits >> 16);
    ctx->buf[62] = (uint8_t)(total_bits >> 8);  ctx->buf[63] = (uint8_t)(total_bits);
    sha256_transform(ctx, ctx->buf);
    for (int i = 0; i < 8; i++) {
        out[i*4]   = (uint8_t)(ctx->h[i] >> 24);
        out[i*4+1] = (uint8_t)(ctx->h[i] >> 16);
        out[i*4+2] = (uint8_t)(ctx->h[i] >> 8);
        out[i*4+3] = (uint8_t)(ctx->h[i]);
    }
}

char *rt_crypto_sha256(const char *data, int64_t len) {
    SHA256Ctx ctx;
    sha256_init(&ctx);
    if (data && len > 0) sha256_update(&ctx, (const uint8_t *)data, (size_t)len);
    uint8_t hash[32];
    sha256_final(&ctx, hash);
    return rt_managed_from_slice((const char *)hash, 32);
}

char *rt_crypto_sha256_hex(const char *data, int64_t len) {
    SHA256Ctx ctx;
    sha256_init(&ctx);
    if (data && len > 0) sha256_update(&ctx, (const uint8_t *)data, (size_t)len);
    uint8_t hash[32];
    sha256_final(&ctx, hash);
    char hex[65];
    static const char hexchars[] = "0123456789abcdef";
    for (int i = 0; i < 32; i++) {
        hex[i*2]   = hexchars[(hash[i] >> 4) & 0xf];
        hex[i*2+1] = hexchars[hash[i] & 0xf];
    }
    hex[64] = '\0';
    return rt_managed_from_slice(hex, 64);
}

// ═══════════════════════════════════════════════════════════════════════
//  HMAC-SHA256
// ═══════════════════════════════════════════════════════════════════════

static void sha256_hmac(const uint8_t *key, size_t key_len,
                        const uint8_t *msg, size_t msg_len,
                        uint8_t out[32])
{
    uint8_t k_pad[64];
    memset(k_pad, 0, 64);
    if (key_len > 64) {
        SHA256Ctx kctx;
        sha256_init(&kctx);
        sha256_update(&kctx, key, key_len);
        uint8_t k_hash[32];
        sha256_final(&kctx, k_hash);
        memcpy(k_pad, k_hash, 32);
    } else {
        memcpy(k_pad, key, key_len);
    }

    uint8_t o_key_pad[64], i_key_pad[64];
    for (int i = 0; i < 64; i++) {
        o_key_pad[i] = k_pad[i] ^ 0x5c;
        i_key_pad[i] = k_pad[i] ^ 0x36;
    }

    SHA256Ctx ictx;
    sha256_init(&ictx);
    sha256_update(&ictx, i_key_pad, 64);
    sha256_update(&ictx, msg, msg_len);
    uint8_t i_hash[32];
    sha256_final(&ictx, i_hash);

    SHA256Ctx octx;
    sha256_init(&octx);
    sha256_update(&octx, o_key_pad, 64);
    sha256_update(&octx, i_hash, 32);
    sha256_final(&octx, out);
}

char *rt_crypto_hmac_sha256(const char *key, int64_t key_len,
                            const char *data, int64_t data_len)
{
    uint8_t mac[32];
    sha256_hmac((const uint8_t *)key, (size_t)key_len,
                (const uint8_t *)data, (size_t)data_len, mac);
    return rt_managed_from_slice((const char *)mac, 32);
}

char *rt_crypto_hmac_sha256_hex(const char *key, int64_t key_len,
                                const char *data, int64_t data_len)
{
    uint8_t mac[32];
    sha256_hmac((const uint8_t *)key, (size_t)key_len,
                (const uint8_t *)data, (size_t)data_len, mac);
    char hex[65];
    static const char hexchars[] = "0123456789abcdef";
    for (int i = 0; i < 32; i++) {
        hex[i*2]   = hexchars[(mac[i] >> 4) & 0xf];
        hex[i*2+1] = hexchars[mac[i] & 0xf];
    }
    hex[64] = '\0';
    return rt_managed_from_slice(hex, 64);
}

// ═══════════════════════════════════════════════════════════════════════
//  Base64 encoding / decoding
// ═══════════════════════════════════════════════════════════════════════

static const char b64_table[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

char *rt_crypto_base64_encode(const char *data, int64_t len) {
    if (!data || len <= 0) return rt_managed_from_slice("", 0);
    size_t in_len = (size_t)len;
    size_t out_len = 4 * ((in_len + 2) / 3);
    char *out = rt_managed_alloc(out_len);
    if (!out) return rt_managed_from_slice("", 0);
    size_t o = 0;
    const uint8_t *s = (const uint8_t *)data;
    for (size_t i = 0; i < in_len; i += 3) {
        uint32_t n = (uint32_t)s[i] << 16;
        if (i + 1 < in_len) n |= (uint32_t)s[i+1] << 8;
        if (i + 2 < in_len) n |= (uint32_t)s[i+2];
        out[o++] = b64_table[(n >> 18) & 0x3F];
        out[o++] = b64_table[(n >> 12) & 0x3F];
        out[o++] = (i + 1 < in_len) ? b64_table[(n >> 6) & 0x3F] : '=';
        out[o++] = (i + 2 < in_len) ? b64_table[n & 0x3F] : '=';
    }
    out[o] = '\0';
    return out;
}

static int8_t b64_decode_char(char c) {
    if (c >= 'A' && c <= 'Z') return (int8_t)(c - 'A');
    if (c >= 'a' && c <= 'z') return (int8_t)(c - 'a' + 26);
    if (c >= '0' && c <= '9') return (int8_t)(c - '0' + 52);
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

char *rt_crypto_base64_decode(const char *data, int64_t len) {
    if (!data || len <= 0) return rt_managed_from_slice("", 0);
    size_t in_len = (size_t)len;
    size_t out_cap = in_len * 3 / 4 + 4;
    uint8_t *out = (uint8_t *)malloc(out_cap);
    if (!out) return rt_managed_from_slice("", 0);
    size_t o = 0;
    uint32_t acc = 0;
    int bits = 0;
    for (size_t i = 0; i < in_len; i++) {
        if (data[i] == '=' || data[i] == '\n' || data[i] == '\r') continue;
        int8_t val = b64_decode_char(data[i]);
        if (val < 0) continue;
        acc = (acc << 6) | (uint32_t)val;
        bits += 6;
        if (bits >= 8) {
            bits -= 8;
            out[o++] = (uint8_t)((acc >> bits) & 0xFF);
        }
    }
    char *result = rt_managed_from_slice((const char *)out, o);
    free(out);
    return result;
}

// ═══════════════════════════════════════════════════════════════════════
//  Hex encoding / decoding
// ═══════════════════════════════════════════════════════════════════════

static const char hex_table[] = "0123456789abcdef";

char *rt_crypto_hex_encode(const char *data, int64_t len) {
    if (!data || len <= 0) return rt_managed_from_slice("", 0);
    size_t in_len = (size_t)len;
    size_t out_len = in_len * 2;
    char *out = rt_managed_alloc(out_len);
    if (!out) return rt_managed_from_slice("", 0);
    const uint8_t *s = (const uint8_t *)data;
    for (size_t i = 0; i < in_len; i++) {
        out[i*2]   = hex_table[(s[i] >> 4) & 0xf];
        out[i*2+1] = hex_table[s[i] & 0xf];
    }
    out[out_len] = '\0';
    return out;
}

static int hex_digit_val(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

char *rt_crypto_hex_decode(const char *hex, int64_t len) {
    if (!hex || len <= 0) return rt_managed_from_slice("", 0);
    size_t in_len = (size_t)len;
    size_t out_len = in_len / 2;
    char *out = rt_managed_alloc(out_len);
    if (!out) return rt_managed_from_slice("", 0);
    size_t o = 0;
    for (size_t i = 0; i + 1 < in_len; i += 2) {
        int hi = hex_digit_val(hex[i]);
        int lo = hex_digit_val(hex[i+1]);
        if (hi < 0 || lo < 0) continue;
        out[o++] = (char)((hi << 4) | lo);
    }
    return out;
}

// ═══════════════════════════════════════════════════════════════════════
//  Legacy crypto helpers (kept for backward compatibility)
// ═══════════════════════════════════════════════════════════════════════

int64_t rt_crypto_byte_at(const char *s, int64_t i) {
    if (!s || i < 0) return 0;
    return (unsigned char)s[i];
}

char *rt_read_bytes(const char *path) {
    if (!path) return rt_managed_from_slice("", 0);
    FILE *fh = fopen(path, "rb");
    if (!fh) return rt_managed_from_slice("", 0);
    fseek(fh, 0, SEEK_END);
    long size = ftell(fh);
    fseek(fh, 0, SEEK_SET);
    if (size <= 0) { fclose(fh); return rt_managed_from_slice("", 0); }
    char *result = rt_managed_alloc((size_t)size);
    if (!result) { fclose(fh); return rt_managed_from_slice("", 0); }
    fread(result, 1, (size_t)size, fh);
    result[size] = '\0';
    fclose(fh);
    return result;
}

int rt_hex_to_file(const char *path, const char *hex) {
    if (!path || !hex) return 0;
    FILE *fh = fopen(path, "wb");
    if (!fh) return 0;
    size_t len = strlen(hex);
    for (size_t i = 0; i + 1 < len; i += 2) {
        int hi = hex_digit_val(hex[i]);
        int lo = hex_digit_val(hex[i + 1]);
        if (hi < 0 || lo < 0) continue;
        unsigned char byte = (unsigned char)((hi << 4) | lo);
        fwrite(&byte, 1, 1, fh);
    }
    fclose(fh);
    return 1;
}
