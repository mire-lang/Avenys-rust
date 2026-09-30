#ifndef MIRE_PAL_CRYPTO_H
#define MIRE_PAL_CRYPTO_H

#include <stddef.h>
#include <stdint.h>
#include "pal.h"

// ── Ed25519 ──────────────────────────────────────────────────────
// buffer sizes (from libsodium/crypto_sign_ed25519.h)
#define PAL_CRYPTO_ED25519_BYTES             64
#define PAL_CRYPTO_ED25519_PUBLICKEYBYTES    32
#define PAL_CRYPTO_ED25519_SECRETKEYBYTES    64
#define PAL_CRYPTO_SHA256_BYTES              32
#define PAL_CRYPTO_SHA512_BYTES              64

pal_error_code_t pal_crypto_sha256(const unsigned char *input, size_t len,
                                   unsigned char *output);
pal_error_code_t pal_crypto_sha512(const unsigned char *input, size_t len,
                                   unsigned char *output);
pal_error_code_t pal_crypto_random_bytes(void *buf, size_t len);

// All functions return pal_error_code_t (PAL_ERR_OK == 0 on success, a
// non-zero error otherwise). This mirrors the libsodium `0 / non-zero`
// contract while using the PAL's unified error type.

pal_error_code_t pal_crypto_ed25519_keypair(unsigned char *public_key, unsigned char *secret_key);
pal_error_code_t pal_crypto_ed25519_sign(
    unsigned char *signature,
    const unsigned char *msg,
    unsigned long long msg_len,
    const unsigned char *secret_key
);
pal_error_code_t pal_crypto_ed25519_verify(
    const unsigned char *msg,
    unsigned long long msg_len,
    const unsigned char *signature,
    unsigned long long sig_len,
    const unsigned char *public_key
);

#endif // MIRE_PAL_CRYPTO_H
