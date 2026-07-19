//! Golden Corpus — MIR correctness regression suite.
//!
//! For every `<case>.mire` in `tests/golden/`, this suite compiles it with the
//! MIR backend, runs the resulting binary, and compares its stdout/stderr/exit
//! code against the sibling expectation files (`<case>.stdout`, `<case>.stderr`,
//! `<case>.status`). The expectation files are a locked snapshot of the
//! current compiler's behaviour and act as a regression baseline.
//!
//! To add a case: drop `<name>.mire` plus any of `<name>.stdout` /
//! `<name>.stderr` / `<name>.status` you want to assert. If a case is expected
//! to fail compilation, add a `<name>.compile_error` marker file instead.

use mire::error::diagnostic::WarningFilter;
use mire::{
    BuildMode, BuildOptions, ImportMode, OptLevel, compile_file_with_avenys,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

fn unique_temp_root(stem: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "mire_golden_{}_{}",
        stem,
        std::process::id()
    ));
    let mut candidate = base.clone();
    let mut i = 0;
    while candidate.exists() {
        i += 1;
        candidate = base.join(format!("v{i}"));
    }
    fs::create_dir_all(&candidate).expect("create temp project root");
    candidate
}

#[test]
fn golden_corpus() {
    let dir = golden_dir();
    let mut cases: Vec<String> = fs::read_dir(&dir)
        .expect("read golden dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("mire") {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no .mire cases found in {}", dir.display());

    let mut failures = Vec::new();

    for stem in cases {
        let case = dir.join(format!("{stem}.mire"));
        let source = fs::read_to_string(&case).expect("read source");
        let root = unique_temp_root(&stem);
        let source_path = root.join(format!("{stem}.mire"));
        fs::write(&source_path, &source).expect("write source to temp root");

        let options = BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: WarningFilter::Off,
            deny_warnings: HashSet::new(),
            module_paths: vec![],
            test_mode: false,
            ..Default::default()
        };

        match compile_file_with_avenys(&source_path, &options) {
            Ok(build) => {
                let out = Command::new(&build.binary_path)
                    .output()
                    .unwrap_or_else(|e| panic!("run {stem}: {e}"));
                let rc = out.status.code().unwrap_or(-1);

                if let Ok(expected) = fs::read_to_string(dir.join(format!("{stem}.stdout"))) {
                    let got = String::from_utf8_lossy(&out.stdout);
                    if got != expected {
                        failures.push(format!(
                            "[{stem}] stdout mismatch\nexpected:\n{expected}\n---\ngot:\n{got}"
                        ));
                    }
                }
                if let Ok(expected) = fs::read_to_string(dir.join(format!("{stem}.stderr"))) {
                    let got = String::from_utf8_lossy(&out.stderr);
                    if got != expected {
                        failures.push(format!(
                            "[{stem}] stderr mismatch\nexpected:\n{expected}\n---\ngot:\n{got}"
                        ));
                    }
                }
                if let Ok(expected) = fs::read_to_string(dir.join(format!("{stem}.status"))) {
                    let expected = expected.trim().parse::<i32>().unwrap_or(-99);
                    if rc != expected {
                        failures.push(format!("[{stem}] exit code: expected {expected}, got {rc}"));
                    }
                }
            }
            Err(e) => {
                // A compile-error case is expected only if a marker file exists.
                if dir.join(format!("{stem}.compile_error")).exists() {
                    // expected to fail compilation — pass.
                } else {
                    failures.push(format!("[{stem}] compilation failed:\n{e}"));
                }
            }
        }

        let _ = fs::remove_dir_all(&root);
    }

    assert!(failures.is_empty(), "Golden corpus failures:\n{}", failures.join("\n"));
}
