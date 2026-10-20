// Emits MIRE_CODEGEN_EPOCH: a fingerprint of the sources that decide what IR a
// program lowers to.
//
// The incremental cache keys MIR bodies and stores the IR generated from them.
// MIR is unchanged by a change to code generation, so a codegen edit reused IR
// produced by the previous compiler with no warning and no invalidation: the
// build succeeded and the change appeared not to work. Keying on this epoch
// makes a codegen edit a cache miss instead.
//
// Scoped to the lowering and codegen sources rather than the whole crate. A
// comment in the type checker does not change the IR, and invalidating the cache
// for it would cost more than it buys; a change in these directories does.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Directories whose contents can change the generated IR or the analysis
/// result that decides whether a cached build is still valid. Typechecker,
/// parser, and loader changes can alter which programs compile even when the
/// MIR codegen sources are untouched, so they belong in the epoch too.
const IR_INPUTS: &[&str] = &["src/compiler", "src/types", "src/parser", "src/loader"];

/// Individual source files outside `IR_INPUTS` that also change the IR.
const IR_FILES: &[&str] = &["src/avens/build_support.rs"];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut items: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    // Directory order from the filesystem is not stable across machines.
    items.sort();
    for path in items {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in IR_INPUTS {
        collect(&root.join(dir), &mut files);
    }
    for file in IR_FILES {
        files.push(root.join(file));
    }
    files.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for path in &files {
        // Hash the path relative to the manifest so a checkout in a different
        // directory produces the same epoch.
        if let Ok(rel) = path.strip_prefix(&root) {
            rel.to_string_lossy().hash(&mut hasher);
        }
        match std::fs::read(path) {
            Ok(bytes) => {
                bytes.hash(&mut hasher);
            }
            Err(_) => {
                // A source we cannot read still changes the fingerprint: better
                // to miss the cache once than to serve stale IR.
                b"<unreadable>".hash(&mut hasher);
            }
        }
    }
    let epoch = format!("{:016x}", hasher.finish());
    println!("cargo:rustc-env=MIRE_CODEGEN_EPOCH={epoch}");
    for dir in IR_INPUTS {
        println!("cargo:rerun-if-changed={dir}");
    }
    for file in IR_FILES {
        println!("cargo:rerun-if-changed={file}");
    }
}
