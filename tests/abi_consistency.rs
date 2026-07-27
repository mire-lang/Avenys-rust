//! ABI consistency test.
//!
//! Validates `docs/abi_map.toml` — the authoritative catalogue of LLVM symbols
//! emitted by the MIR codegen — against the C implementation:
//!
//! 1. Every catalogued symbol is defined in its C source
//!    (`src/pal/linux` for `pal_*`, `src/runtime` for `rt_*`/`ireru`).
//! 2. The catalogue is complete and not stale: it matches exactly the set of
//!    `pal_*`/`rt_*`/`ireru` symbols declared by the codegen in
//!    `src/compiler/mir/codegen/builtins.rs`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Meta {
    symbol_count: usize,
}

#[derive(Deserialize)]
struct Symbol {
    target: String,
    category: String,
}

#[derive(Deserialize)]
struct AbiMap {
    meta: Meta,
    #[serde(rename = "symbol")]
    symbols: std::collections::HashMap<String, Symbol>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read every `.c` file under `dir` (recursively) into a single string.
fn read_c_sources(dir: &Path) -> String {
    let mut out = String::new();
    fn walk(dir: &Path, out: &mut String) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("c") {
                    if let Ok(src) = fs::read_to_string(&path) {
                        out.push_str(&src);
                        out.push('\n');
                    }
                }
            }
        }
    }
    walk(dir, &mut out);
    out
}

/// True if `source` contains a definition/declaration `sym(` not preceded by an
/// identifier character (so `pal_fs_write(` matches but `xpal_fs_write(` does not).
fn source_defines(source: &str, sym: &str) -> bool {
    let pat = format!("{}(", sym);
    let bytes = source.as_bytes();
    let mut start = 0;
    while let Some(rel) = source[start..].find(&pat) {
        let abs = start + rel;
        let prev = if abs == 0 {
            b'_'
        } else {
            bytes[abs - 1]
        };
        let prev_ok = !prev.is_ascii_alphanumeric() && prev != b'_';
        if prev_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Symbols the MIR codegen actually declares (the real ABI surface).
fn emitted_symbols() -> HashSet<String> {
    let path = root().join("src/compiler/mir/codegen/builtins.rs");
    let src = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    let mut set = HashSet::new();
    for line in src.lines() {
        if let Some(idx) = line.find("declare") {
            if let Some(at) = line[idx..].find('@') {
                let after = &line[idx + at + 1..];
                let sym = after
                    .split(|c: char| c == '(' || c.is_whitespace())
                    .next()
                    .unwrap_or("");
                if sym.starts_with("pal_") || sym.starts_with("rt_") || sym == "ireru" {
                    set.insert(sym.to_string());
                }
            }
        }
    }
    set
}

#[test]
fn abi_catalogue_matches_codegen_and_c_sources() {
    let toml_path = root().join("docs/abi_map.toml");
    let toml_src = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", toml_path.display(), e));
    let map: AbiMap = toml::from_str(&toml_src)
        .unwrap_or_else(|e| panic!("cannot parse {}: {}", toml_path.display(), e));

    assert_eq!(
        map.meta.symbol_count,
        map.symbols.len(),
        "meta.symbol_count disagrees with the number of [symbol.*] entries"
    );

    let core = read_c_sources(&root().join("src/pal/core"));
    let linux = read_c_sources(&root().join("src/pal/linux"));
    let runtime = read_c_sources(&root().join("src/runtime"));

    let emitted = emitted_symbols();

    for (name, sym) in &map.symbols {
        assert_eq!(name, &sym.target, "entry key `{name}` must equal target");

        match sym.category.as_str() {
            "pal" => {
                // pal_* symbols can be in core (error, alloc) or linux (backend)
                let in_core = source_defines(&core, &sym.target);
                let in_linux = source_defines(&linux, &sym.target);
                assert!(
                    in_core || in_linux,
                    "pal symbol `{}` is not defined in src/pal/core or src/pal/linux",
                    sym.target
                );
            }
            "runtime" | "ireru" => {
                assert!(
                    source_defines(&runtime, &sym.target),
                    "{} symbol `{}` is not defined in src/runtime",
                    sym.category,
                    sym.target
                );
            }
            "libc" => {
                // libc symbols are provided by the system; no source check needed
            }
            other => panic!("unknown category `{other}` for symbol `{}`", sym.target),
        }

        // No stale entries: every catalogued symbol must still be emitted.
        assert!(
            emitted.contains(&sym.target),
            "catalogued symbol `{}` is not declared by the codegen (stale entry)",
            sym.target
        );
    }

    // Completeness: every emitted symbol must be catalogued.
    for sym in &emitted {
        assert!(
            map.symbols.values().any(|s| &s.target == sym),
            "emitted symbol `{sym}` is missing from docs/abi_map.toml"
        );
    }
}
