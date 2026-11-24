// SHA-256 verification for `-L name=path -H sha256:<hex>`.
//
// Avenys does not resolve dependencies, so the only thing it can check is that
// the path the orchestrator handed it still has the content the orchestrator
// pinned. The digest is over a file's bytes, or over a directory tree in
// deterministic order (relative path, NUL, file bytes). Build/VCS noise is
// excluded because it changes without the library changing.

use crate::cli::CommonOptions;
use mire::error::diagnostic::DiagnosticCode;
use mire::MireError;
use std::fs;
use std::path::{Component, Path, PathBuf};

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
    0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
    0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
    0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
    0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
    0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut chunks = data.chunks_exact(64);
    for block in &mut chunks {
        compress(&mut state, block);
    }
    let rem = chunks.remainder();
    let mut tail = Vec::with_capacity(128);
    tail.extend_from_slice(rem);
    tail.push(0x80);
    while (tail.len() % 64) != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in tail.chunks_exact(64) {
        compress(&mut state, block);
    }

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn skip_dir(name: &str) -> bool {
    matches!(name, ".git" | "bin" | "target" | ".cache" | "logs" | "node_modules")
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !skip_dir(&name) {
                collect_files(root, &path, files);
            }
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .filter_map(|component| match component {
                    Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            files.push((rel, path));
        }
    }
}

fn digest_path(path: &Path) -> std::io::Result<String> {
    if path.is_file() {
        return Ok(format!("sha256:{}", hex(&sha256(&fs::read(path)?))));
    }

    let mut files = Vec::new();
    collect_files(path, path, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut stream = Vec::new();
    for (rel, file) in files {
        stream.extend_from_slice(rel.as_bytes());
        stream.push(0);
        stream.extend_from_slice(&fs::read(file)?);
    }
    Ok(format!("sha256:{}", hex(&sha256(&stream))))
}

/// Verify every `-H` against the library named by the preceding `--link`.
/// The flag exists so an orchestrator can pin a path without trusting it.
pub(crate) fn verify_ecosystem_hashes(common: &CommonOptions) -> Result<(), MireError> {
    for (index, expected) in common.ecosystem_hashes.iter().enumerate() {
        let Some(expected) = expected else {
            continue;
        };
        let (name, path) = &common.ecosystem_libs[index];
        let path = expand_tilde(path);
        let actual = digest_path(&path).map_err(|err| {
            MireError::native_dependency(
                format!(
                    "cannot read library `{name}` at `{}` to verify `{expected}`: {err}",
                    path.display()
                ),
                DiagnosticCode::E0403,
            )
        })?;
        if actual != *expected {
            return Err(MireError::native_dependency(
                format!(
                    "hash mismatch for library `{name}`\n  = expected: {expected}\n  = actual:   {actual}\n  = path:     {}",
                    path.display()
                ),
                DiagnosticCode::E0403,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn directory_digest_is_order_independent_and_content_sensitive() {
        let root = std::env::temp_dir().join(format!("mire-hash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).expect("mkdir");
        fs::write(root.join("b.txt"), b"second").expect("write b");
        fs::write(root.join("sub/a.txt"), b"first").expect("write a");

        let first = digest_path(&root).expect("digest");

        // Rewriting files (which changes mtimes but not contents) must not move
        // the digest; a directory is pinned by content, not by metadata.
        fs::write(root.join("b.txt"), b"second").expect("rewrite b");
        assert_eq!(digest_path(&root).expect("digest again"), first);

        fs::write(root.join("b.txt"), b"changed").expect("change b");
        assert_ne!(digest_path(&root).expect("digest changed"), first);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn file_digest_matches_the_raw_bytes() {
        let path = std::env::temp_dir().join(format!("mire-hash-file-{}", std::process::id()));
        fs::write(&path, b"abc").expect("write");
        assert_eq!(
            digest_path(&path).expect("digest"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_file(&path);
    }
}
