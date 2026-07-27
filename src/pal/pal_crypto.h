#ifndef MIRE_PAL_CRYPTO_H
#define MIRE_PAL_CRYPTO_H

#include <stddef.h>
#include <stdint.h>

// ── Ed25519 ──────────────────────────────────────────────────────
// buffer sizes (from libsodium/crypto_sign_ed25519.h)
#define PAL_CRYPTO_ED25519_BYTES             64
#define PAL_CRYPTO_ED25519_PUBLICKEYBYTES    32
#define PAL_CRYPTO_ED25519_SECRETKEYBYTES    64

int pal_crypto_ed25519_keypair(unsigned char *public_key, unsigned char *secret_key);
int pal_crypto_ed25519_sign(
    unsigned char *signature,
    const unsigned char *msg,
    unsigned long long msg_len,
    const unsigned char *secret_key
);
int pal_crypto_ed25519_verify(
    const unsigned char *msg,
    unsigned long long msg_len,
    const unsigned char *signature,
    unsigned long long sig_len,
    const unsigned char *public_key
);

#endif // MIRE_PAL_CRYPTO_H
