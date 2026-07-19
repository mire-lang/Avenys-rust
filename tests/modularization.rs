//! Modularization regression tests.
//!
//! Phase A split `src/main.rs` into `src/cli/` and Phase B split
//! `src/loader.rs` into `src/loader/`. These tests lock the refactor by
//! driving the loader's public entry points (now owned by the `loader/`
//! module tree) and asserting the load → compile pipeline still produces a
//! correct program. The CLI split (binary crate) is covered indirectly by the
//! golden corpus and `cargo test --lib`, which exercise the same code paths.

use mire::{
    BuildMode, BuildOptions, ImportMode, MireError, OptLevel,
    compile_file_with_avenys, load_program_with_metadata,
};
use mire::error::diagnostic::WarningFilter;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("loads")
        .join(name)
}

#[test]
fn loader_entry_point_resolves_program() {
    // Exercises resolver.rs + renamer.rs + source.rs after the split.
    let path = fixture("code/main.mire");
    let loaded = load_program_with_metadata(&path)
        .expect("load_program_with_metadata should succeed for code/main.mire");

    assert!(
        !loaded.program.statements.is_empty(),
        "loaded program must contain statements"
    );

    let has_main = loaded.program.statements.iter().any(|s| matches!(
        s,
        mire::parser::ast::Statement::Function { name, .. } if name == "main"
    ));
    assert!(has_main, "code/main.mire must expose pub fn main");

    // Each statement must carry an origin recorded by the loader.
    assert_eq!(
        loaded.statement_origins.len(),
        loaded.program.statements.len(),
        "every statement must have a recorded origin after the split"
    );
}

#[test]
fn loader_resolves_nested_module() {
    // Exercises the renamer/scoped-prefix path on a standalone module file.
    let path = fixture("nested/mod.mire");
    let loaded = load_program_with_metadata(&path)
        .expect("load_program_with_metadata should succeed for nested/mod.mire");

    let exported: Vec<String> = loaded
        .program
        .statements
        .iter()
        .filter_map(mire::incremental::statement_export_name)
        .map(|s| s.to_string())
        .collect();
    assert!(
        exported.iter().any(|n| n == "from_nested"),
        "nested/mod.mire must export from_nested, got {exported:?}"
    );
}

#[test]
fn loader_to_compiler_pipeline_intact() -> Result<(), MireError> {
    // End-to-end: load (loader module) then compile (compiler module). This is
    // the exact path the CLI `run`/`build` commands use; if the loader split
    // had broken expansion, compilation would fail or produce wrong code.
    let path = fixture("code/main.mire");
    let _loaded = load_program_with_metadata(&path)?;

    let options = BuildOptions {
        mode: BuildMode::Debug,
        opt_level: OptLevel::O0,
        output: None,
        emit_binary: false,
        persist_ir: true,
        import_mode: ImportMode::Reachable,
        cache: Default::default(),
        warning_filter: WarningFilter::Off,
        deny_warnings: HashSet::new(),
        test_mode: false,
        module_paths: vec![],
        ..Default::default()
    };

    let build = compile_file_with_avenys(&path, &options)?;
    assert!(
        build.ir_path.is_some() || build.binary_path.exists(),
        "loader→compiler pipeline must produce output"
    );
    Ok(())
}
