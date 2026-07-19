//! `load!` / `use!` integration tests.
//!
//! Drives the full load → typeck → compile pipeline against small on-disk
//! projects to lock the `load!` local module exposure and the mandatory `use!`
//! wrapper enforcement.

use mire::{
    BuildMode, BuildOptions, ImportMode, OptLevel, analyze_program, compile_file_with_avenys,
    load_program_with_metadata,
};
use mire::error::diagnostic::WarningFilter;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_project(stem: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("mire_loadlocal_{}_{}", stem, std::process::id()));
    let mut candidate = base.clone();
    let mut i = 0;
    while candidate.exists() {
        i += 1;
        candidate = base.join(format!("v{i}"));
    }
    fs::create_dir_all(&candidate).expect("create temp project root");
    candidate
}

fn write_project(root: &PathBuf, main_rel: &str, main_src: &str) {
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"lt\"\nversion = \"0.1.0\"\nentry = \"code/main.mire\"\n",
    )
    .expect("write owl.toml");
    let main_path = root.join(main_rel);
    if let Some(parent) = main_path.parent() {
        fs::create_dir_all(parent).expect("create dir");
    }
    fs::write(&main_path, main_src).expect("write main");
}

#[test]
fn load_bang_exposes_module_and_use_macro_runs() {
    let root = temp_project("ok");
    fs::create_dir_all(root.join("math")).expect("mkdir math");
    fs::write(
        root.join("math/main.mire"),
        "pub fn suma: (a :i64 b :i64) :i64 {\n    return a + b\n}\n",
    )
    .expect("write math/main.mire");

    write_project(
        &root,
        "code/main.mire",
        "load! /math\n\npub fn main: () {\n    set r = use! math::suma(2 3)\n    use dasu(r)\n}\n",
    );

    let options = BuildOptions {
        mode: BuildMode::Debug,
        opt_level: OptLevel::O0,
        output: None,
        emit_binary: true,
        persist_ir: false,
        import_mode: ImportMode::Reachable,
        cache: mire::incremental::CacheOverrides {
            analysis_cache: Some(false),
            ..Default::default()
        },
        warning_filter: WarningFilter::Off,
        deny_warnings: HashSet::new(),
        module_paths: vec![],
        test_mode: false,
        ..Default::default()
    };

    let build = compile_file_with_avenys(&root.join("code/main.mire"), &options)
        .expect("load! + use! program should compile");
    let out = Command::new(&build.binary_path)
        .output()
        .expect("run compiled binary");
    assert!(out.status.success(), "binary should exit 0");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "5",
        "use! math::suma(2 3) should print 5"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn load_bang_requires_use_macro() {
    let root = temp_project("no_use");
    fs::create_dir_all(root.join("math")).expect("mkdir math");
    fs::write(
        root.join("math/main.mire"),
        "pub fn suma: (a :i64 b :i64) :i64 {\n    return a + b\n}\n",
    )
    .expect("write math/main.mire");

    write_project(
        &root,
        "code/main.mire",
        "load! /math\n\npub fn main: () {\n    set r = math::suma(2 3)\n    use dasu(r)\n}\n",
    );

    let loaded = load_program_with_metadata(&root.join("code/main.mire"))
        .expect("load should succeed");
    let result = analyze_program(&mut loaded.program.clone(), "");
    assert!(
        result.is_err(),
        "calling a load!-imported symbol without use! must be rejected"
    );
    let msg = format!("{result:?}");
    assert!(
        msg.contains("use!"),
        "error should mention use!: {msg}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn load_bang_rejects_deep_paths() {
    let root = temp_project("deep");
    let deep = root.join("a").join("b").join("c");
    fs::create_dir_all(&deep).expect("mkdir deep");
    fs::write(
        deep.join("ping.mire"),
        "pub fn ping: () :i64 { return 1 }\n",
    )
    .expect("write deep ping.mire");

    write_project(
        &root,
        "code/main.mire",
        "load! /a/b/c/ping\n\npub fn main: () {\n    set r = use! ping::ping()\n    use dasu(r)\n}\n",
    );

    let loaded = load_program_with_metadata(&root.join("code/main.mire"));
    assert!(
        loaded.is_err(),
        "load! descending more than 2 levels below owl.toml must fail"
    );
    let msg = format!("{:?}", loaded.err().unwrap());
    assert!(
        msg.contains("2 levels below owl.toml"),
        "error should mention the depth limit: {msg}"
    );

    let _ = fs::remove_dir_all(&root);
}
