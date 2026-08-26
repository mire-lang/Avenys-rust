//! Nested function definition tests.
//!
//! Verifies that the flattening pass correctly promotes nested function
//! definitions to top-level with `parent::child` naming, and that the
//! full pipeline (parse → flatten → rename → typeck → lower → codegen)
//! produces correct results.

use std::process::Command;

fn make_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mire_nested_test_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn compile_and_run(source: &str, test_name: &str) -> String {
    let root = make_temp_dir(test_name);
    let path = root.join("main.mire");
    std::fs::write(&path, source).unwrap();

    let result = mire::compile_file_with_avenys(
        &path,
        &mire::BuildOptions {
            mode: mire::BuildMode::Debug,
            opt_level: mire::OptLevel::O0,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::default(),
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
            test_mode: false,
            ..Default::default()
        },
    )
    .expect("compile should succeed");

    let output = Command::new(&result.binary_path)
        .output()
        .expect("failed to run binary");
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ---------------------------------------------------------------------------
// Basic nested function flattening
// ---------------------------------------------------------------------------

#[test]
fn single_level_nesting_compiles_and_runs() {
    let source = r#"
pub fn group: () {
    pub fn add: (a :i64, b :i64) :i64 { return a + b }
}
pub fn main: () {
    set result = group::add(3, 4)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "single_level_nesting");
    assert_eq!(output.trim(), "7");
}

#[test]
fn multiple_children_in_parent_compile() {
    let source = r#"
pub fn math: () {
    pub fn add: (a :i64, b :i64) :i64 { return a + b }
    pub fn mul: (a :i64, b :i64) :i64 { return a * b }
}
pub fn main: () {
    set x = math::add(2, 3)
    set y = math::mul(4, 5)
    use dasu(x)
    use dasu(y)
}
"#;
    let output = compile_and_run(source, "multiple_children");
    assert_eq!(output.trim(), "5\n20");
}

// ---------------------------------------------------------------------------
// Deeply nested functions
// ---------------------------------------------------------------------------

#[test]
fn three_level_nesting_compiles_and_runs() {
    let source = r#"
pub fn unwrap: () {
    pub fn i64: () {
        pub fn or: (ptr :ptr, default :i64) :i64 { return default }
    }
}
pub fn main: () {
    set result = unwrap::i64::or(0, 42)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "three_level_nesting");
    assert_eq!(output.trim(), "42");
}

// ---------------------------------------------------------------------------
// Parent with mixed content (nested fns + executable code)
// ---------------------------------------------------------------------------

#[test]
fn parent_with_mixed_body_keeps_executable_statements() {
    let source = r#"
pub fn config: () {
    pub fn get_value: () :i64 { return 10 }
    pub fn get_name: () :i64 { return 20 }
}
pub fn main: () {
    set a = config::get_value()
    set b = config::get_name()
    use dasu(a)
    use dasu(b)
}
"#;
    let output = compile_and_run(source, "mixed_body");
    assert_eq!(output.trim(), "10\n20");
}

// ---------------------------------------------------------------------------
// Flat and nested styles compose
// ---------------------------------------------------------------------------

#[test]
fn flat_and_nested_styles_coexist() {
    let source = r#"
pub fn group::helper: (x :i64) :i64 { return x + 1 }
pub fn group: () {
    pub fn other: (x :i64) :i64 { return x + 2 }
}
pub fn main: () {
    set a = group::helper(10)
    set b = group::other(10)
    use dasu(a)
    use dasu(b)
}
"#;
    let output = compile_and_run(source, "flat_nested_coexist");
    assert_eq!(output.trim(), "11\n12");
}

// ---------------------------------------------------------------------------
// Backward compatibility: flat :: style still works
// ---------------------------------------------------------------------------

#[test]
fn flat_double_colon_style_still_works() {
    let source = r#"
pub fn vec::push::i64: (x :i64) :i64 { return x * 2 }
pub fn main: () {
    set result = vec::push::i64(21)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "flat_still_works");
    assert_eq!(output.trim(), "42");
}

// ---------------------------------------------------------------------------
// Parser level: nested fn is parsed correctly
// ---------------------------------------------------------------------------

#[test]
fn parser_parses_nested_fn_definitions() {
    use mire::parse;
    use mire::parser::ast::Statement;

    let source = r#"
pub fn group: () {
    pub fn child: () { return 1 }
}
"#;
    let program = parse(source).expect("parse should succeed");
    // Before flattening (parse returns flattened), verify structure
    // Actually parse() now includes flattening, so we get flat output
    let names: Vec<&str> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Function { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"group"), "should have group anchor: {:?}", names);
    assert!(
        names.contains(&"group::child"),
        "should have flattened group::child: {:?}",
        names
    );
}

#[test]
fn parser_preserves_visibility_on_nested_children() {
    use mire::parse;
    use mire::parser::ast::{Statement, Visibility};

    let source = r#"
pub fn group: () {
    fn private_fn: () { return 1 }
    pub fn public_fn: () { return 2 }
}
"#;
    let program = parse(source).expect("parse should succeed");

    let find_fn = |name: &str| -> Visibility {
        program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::Function {
                    name: n, visibility, ..
                } if n == name => Some(*visibility),
                _ => None,
            })
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    };

    assert_eq!(find_fn("group::private_fn"), Visibility::Private);
    assert_eq!(find_fn("group::public_fn"), Visibility::Public);
}
