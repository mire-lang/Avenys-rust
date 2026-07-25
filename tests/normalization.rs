//! Normalization pipeline tests.
//!
//! These tests verify that the `::` → `.` conversion happens exactly once at the
//! correct compiler boundary, and that downstream passes never touch the separator
//! after normalization.

use mire::parse;
use mire::parser::ast::{Expression, Statement};
use std::process::Command;

// ---------------------------------------------------------------------------
// Parser level: `::` preserved in declarations, normalized in call expressions
// ---------------------------------------------------------------------------

#[test]
fn parser_preserves_double_colon_in_fn_declaration() {
    let source = "fn push::i64: (x :i64) { }\n";
    let program = parse(source).expect("parse should succeed");
    let Statement::Function { name, .. } = &program.statements[0] else {
        panic!("expected Function statement");
    };
    assert_eq!(name, "push::i64", "parser must preserve :: in fn declaration names");
}

#[test]
fn parser_preserves_double_colon_in_pub_fn_declaration() {
    let source = "pub fn vec::push::i64: (x :i64) { }\n";
    let program = parse(source).expect("parse should succeed");
    let Statement::Function { name, .. } = &program.statements[0] else {
        panic!("expected Function statement");
    };
    assert_eq!(name, "vec::push::i64", "parser must preserve :: in pub fn declaration names");
}

#[test]
fn parser_normalizes_double_colon_to_dot_in_call_expressions() {
    let source = "fn foo: () { vec::push::i64(data, 0) }\n";
    let program = parse(source).expect("parse should succeed");
    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected Function statement");
    };
    let Statement::Expression(Expression::Call { name, .. }) = &body[0] else {
        panic!("expected Call expression in body");
    };
    assert_eq!(name, "vec.push.i64", "parser must normalize :: to . in call expressions");
}

#[test]
fn parser_normalizes_dot_call_expressions() {
    let source = "fn foo: () { vec.push.i64(data, 0) }\n";
    let program = parse(source).expect("parse should succeed");
    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected Function statement");
    };
    let Statement::Expression(Expression::Call { name, .. }) = &body[0] else {
        panic!("expected Call expression in body");
    };
    assert_eq!(name, "vec.push.i64", "parser must normalize . calls consistently");
}

#[test]
fn parser_preserves_double_colon_in_fn_with_return_type() {
    let source = "fn to::str: (x :i64) :str { return \"hi\" }\n";
    let program = parse(source).expect("parse should succeed");
    let Statement::Function { name, .. } = &program.statements[0] else {
        panic!("expected Function statement");
    };
    assert_eq!(name, "to::str", "parser must preserve :: in to::str declaration");
}

// ---------------------------------------------------------------------------
// canonical_fn_name utility
// ---------------------------------------------------------------------------

#[test]
fn canonical_fn_name_normalizes_correctly() {
    assert_eq!(mire::canonical_fn_name("push::i64"), "push.i64");
    assert_eq!(mire::canonical_fn_name("vec.push.i64"), "vec.push.i64", "dots should not change");
    assert_eq!(mire::canonical_fn_name("a::b::c"), "a.b.c");
    assert_eq!(mire::canonical_fn_name("plain"), "plain", "no separators unchanged");
    assert_eq!(mire::canonical_fn_name("is::some"), "is.some");
    assert_eq!(mire::canonical_fn_name(""), "", "empty string unchanged");
}

// ---------------------------------------------------------------------------
// Full pipeline: fn declaration with :: compiles and runs
// ---------------------------------------------------------------------------

fn make_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mire_norm_test_{}", name));
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

#[test]
fn fn_with_double_colon_compiles_and_runs() {
    let source = r#"
fn add::i64: (a :i64, b :i64) :i64 {
    return a + b
}
pub fn main: () {
    set result = add::i64(3, 4)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "double_colon_fn");
    assert_eq!(output.trim(), "7");
}

#[test]
fn fn_with_double_colon_call_expression_compiles() {
    let source = r#"
fn vec::helper::i64: (x :i64) :i64 {
    return x * 2
}
pub fn main: () {
    set result = vec::helper::i64(21)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "double_colon_call");
    assert_eq!(output.trim(), "42");
}

#[test]
fn mixed_dot_and_double_colon_calls_compile() {
    let source = r#"
fn math::add::i64: (a :i64, b :i64) :i64 {
    return a + b
}
pub fn main: () {
    set x = math.add.i64(10, 20)
    set y = math::add::i64(10, 20)
    use dasu(x)
    use dasu(y)
}
"#;
    let output = compile_and_run(source, "mixed_dot_double_colon");
    assert_eq!(output.trim(), "30\n30");
}

#[test]
fn nested_double_colon_chain_compiles() {
    let source = r#"
fn my::helper::i64: (x :i64) :i64 {
    return x + 1
}
pub fn main: () {
    set result = my::helper::i64(99)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "nested_double_colon_chain");
    assert_eq!(output.trim(), "100");
}

#[test]
fn double_colon_declaration_called_with_dot_syntax() {
    let source = r#"
fn my::func::i64: (x :i64) :i64 {
    return x + 10
}
pub fn main: () {
    set result = my.func.i64(5)
    use dasu(result)
}
"#;
    let output = compile_and_run(source, "decl_double_colon_called_dot");
    assert_eq!(output.trim(), "15");
}
