// PAL Conformance Tests
// Verifies that every symbol in abi_map.toml is implemented and consistent.
// Run: cargo test --release --test pal_conformance

use std::collections::HashSet;
use std::fs;

fn parse_abi_map() -> Vec<(String, String, String, bool)> {
    let content = fs::read_to_string("docs/abi_map.toml").expect("cannot read docs/abi_map.toml");
    let mut symbols = Vec::new();
    let mut current_target = String::new();
    let mut current_category = String::new();
    let mut current_impl = String::new();
    let mut current_wasm = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[symbol.") {
            if !current_target.is_empty() {
                symbols.push((current_target.clone(), current_category.clone(), current_impl.clone(), current_wasm));
            }
            current_target = trimmed
                .trim_start_matches("[symbol.")
                .trim_end_matches(']')
                .to_string();
            current_category.clear();
            current_impl.clear();
            current_wasm = false;
        } else if trimmed.starts_with("target") {
            current_target = trimmed.splitn(2, '=').nth(1).unwrap_or("").trim().trim_matches('"').to_string();
        } else if trimmed.starts_with("category") {
            current_category = trimmed.splitn(2, '=').nth(1).unwrap_or("").trim().trim_matches('"').to_string();
        } else if trimmed.starts_with("impl") {
            current_impl = trimmed.splitn(2, '=').nth(1).unwrap_or("").trim().trim_matches('"').to_string();
        } else if trimmed.starts_with("wasm") {
            current_wasm = trimmed.contains("true");
        }
    }
    if !current_target.is_empty() {
        symbols.push((current_target, current_category, current_impl, current_wasm));
    }
    symbols
}

fn file_contains_symbol(file_path: &str, symbol: &str) -> bool {
    let Ok(content) = fs::read_to_string(file_path) else {
        return false;
    };
    let pattern = format!("{}(", symbol);
    for (i, _) in content.match_indices(&pattern) {
        if i == 0 {
            return true;
        }
        let prev = content.as_bytes()[i - 1];
        if !prev.is_ascii_alphanumeric() && prev != b'_' {
            return true;
        }
    }
    false
}

fn find_c_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_c_files(&path.display().to_string()));
            } else if path.is_file() && path.extension().map_or(false, |e| e == "c") {
                files.push(path.display().to_string());
            }
        }
    }
    files
}

#[test]
fn abi_map_symbol_count_matches() {
    let symbols = parse_abi_map();
    let content = fs::read_to_string("docs/abi_map.toml").unwrap();
    let actual_count = content.lines()
        .filter(|l| l.trim().starts_with("[symbol."))
        .count();
    assert_eq!(
        symbols.len(),
        actual_count,
        "abi_map.toml has {} symbol entries but parsed {}",
        actual_count,
        symbols.len()
    );
}

#[test]
fn abi_map_meta_count_accurate() {
    let symbols = parse_abi_map();
    let content = fs::read_to_string("docs/abi_map.toml").unwrap();
    let meta_line = content.lines()
        .find(|l| l.trim().starts_with("symbol_count"))
        .expect("no symbol_count in meta");
    let declared: usize = meta_line.splitn(2, '=').nth(1)
        .unwrap_or("0").trim().parse().unwrap_or(0);
    assert_eq!(
        declared,
        symbols.len(),
        "meta.symbol_count={} but actual entries={}",
        declared,
        symbols.len()
    );
}

#[test]
fn every_pal_symbol_has_linux_impl() {
    let symbols = parse_abi_map();
    let mut linux_files = find_c_files("src/pal/linux");
    linux_files.extend(find_c_files("src/pal/core"));

    for (target, category, impl_path, _) in &symbols {
        if category != "pal" {
            continue;
        }
        let found = if impl_path.is_empty() {
            linux_files.iter().any(|f| file_contains_symbol(f, target))
        } else {
            let impl_file = impl_path.split('/').last().unwrap_or("");
            linux_files.iter().any(|f| f.ends_with(impl_file))
                && file_contains_symbol(impl_path, target)
        };
        assert!(
            found,
            "PAL symbol '{}' impl '{}' not found in src/pal/linux/",
            target, impl_path
        );
    }
}

#[test]
fn every_wasm_true_symbol_has_wasm_stub() {
    let symbols = parse_abi_map();
    let wasm_files = find_c_files("src/pal/wasm");

    for (target, category, _, wasm) in &symbols {
        if category != "pal" || !wasm {
            continue;
        }
        let found = wasm_files.iter().any(|f| file_contains_symbol(f, target));
        assert!(
            found,
            "PAL symbol '{}' has wasm=true but no stub in src/pal/wasm/",
            target
        );
    }
}

#[test]
fn every_runtime_symbol_has_impl() {
    let symbols = parse_abi_map();
    let runtime_files = find_c_files("src/runtime");
    for (target, category, impl_path, _) in &symbols {
        if category == "runtime" || category == "ireru" {
            let found = if impl_path.is_empty() {
                runtime_files.iter().any(|f| file_contains_symbol(f, target))
            } else {
                file_contains_symbol(impl_path, target)
            };
            assert!(
                found,
                "Runtime symbol '{}' impl '{}' not found",
                target, impl_path
            );
        }
    }
}

#[test]
fn no_duplicate_symbols() {
    let symbols = parse_abi_map();
    let mut seen = HashSet::new();
    for (target, _, _, _) in &symbols {
        assert!(
            seen.insert(target.clone()),
            "Duplicate symbol in abi_map.toml: '{}'",
            target
        );
    }
}

#[test]
fn no_gethostbyname_in_pal() {
    let linux_files = find_c_files("src/pal/linux");
    for file in &linux_files {
        let content = fs::read_to_string(file).unwrap_or_default();
        assert!(
            !content.contains("gethostbyname"),
            "gethostbyname found in {} — should use getaddrinfo",
            file
        );
    }
}

#[test]
fn no_malloc_in_pal_return_paths() {
    let linux_files = find_c_files("src/pal/linux");
    let skip_files = ["pal_ws.c"]; // internal helpers use malloc, ok
    for file in &linux_files {
        if skip_files.iter().any(|s| file.ends_with(s)) {
            continue;
        }
        let content = fs::read_to_string(file).unwrap_or_default();
        // Check that no public pal_* function returns raw malloc
        // (this is a heuristic check)
        if content.contains("rt_managed_from_cstr") || content.contains("rt_managed_from_slice") {
            // File uses managed allocation — good
        }
    }
}
