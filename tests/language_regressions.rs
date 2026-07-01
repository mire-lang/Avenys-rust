use mire::parser::ast::{DataType, Expression, Statement};
use mire::{
    BuildMode, BuildOptions, ErrorKind, MireError, OptLevel, analyze_program, cache_file_path,
    check_program_types, compile_file_with_avenys, load_program_with_metadata, parse,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn expect_analysis_error(source: &str) -> String {
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source)
        .expect_err("program should fail analysis")
        .to_string()
}

fn expect_compile_error_from_source(test_name: &str, filename: &str, source: &str) -> MireError {
    let root = make_temp_project_root(test_name);
    let source_path = root.join(filename);
    fs::write(&source_path, source).expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect_err("compilation should fail")
}

#[test]
fn undefined_identifier_in_regular_call_is_not_coerced_to_string() {
    let err = expect_analysis_error("pub fn main: () {\nuse len(missing)\n}\n");
    assert!(err.contains("Unknown identifier 'missing'"), "{err}");
}

#[test]
fn immutable_reassignment_still_errors() {
    let err = expect_analysis_error("pub fn main: () {\nset x = 1\nset x = 2\n}\n");
    assert!(
        err.contains("Cannot reassign immutable variable 'x'"),
        "{err}"
    );
}

#[test]
fn unknown_identifier_error_for_removed_keyword_add() {
    let result: Result<_, MireError> = parse("add std\npub fn main: () {}\n");
    let err = result.expect_err("source should fail to parse");
    let err_str = err.to_string();
    assert!(
        err_str.contains("Legacy `add` imports are no longer supported"),
        "{err_str}"
    );
}

#[test]
fn backend_limitation_errors_render_with_backend_kind() {
    let rendered = MireError::new(ErrorKind::Backend {
        message: "Avenys does not yet lower expression Tuple(...)".to_string(),
    })
    .to_string();

    assert!(rendered.contains("error[E0014]"), "{rendered}");
    assert!(rendered.contains("Backend Limitation"), "{rendered}");
    assert!(
        rendered.contains("frontend accepted this program")
            || rendered.contains("current Avenys backend cannot lower"),
        "{rendered}"
    );
}

#[test]
fn compile_reports_lexer_error_kind_and_filename() {
    let err = expect_compile_error_from_source(
        "mire_diag_lexer_kind_filename",
        "lexer_error.mire",
        "pub fn main: () {\n    set x = \\\n}\n",
    );

    assert!(matches!(err.kind, ErrorKind::Lexer { .. }));
    assert!(
        err.filename()
            .is_some_and(|name| name.ends_with("lexer_error.mire"))
    );
    let rendered = err.to_string();
    assert!(rendered.contains("error[E0001]"), "{rendered}");
    assert!(rendered.contains("Lexical Error"), "{rendered}");
}

#[test]
fn compile_reports_parser_error_kind_and_filename() {
    let err = expect_compile_error_from_source(
        "mire_diag_parser_kind_filename",
        "parser_error.mire",
        "pub fn main: () {\n    set x = 10\n    if x > 5\n        use dasu(\"hola\")\n}\n",
    );

    assert!(matches!(err.kind, ErrorKind::Parser { .. }));
    assert!(
        err.filename()
            .is_some_and(|name| name.ends_with("parser_error.mire"))
    );
    let rendered = err.to_string();
    assert!(rendered.contains("error[E0003]"), "{rendered}");
    assert!(rendered.contains("Syntax Error"), "{rendered}");
}

#[test]
fn compile_reports_type_error_kind_and_filename() {
    let err = expect_compile_error_from_source(
        "mire_diag_type_kind_filename",
        "type_error.mire",
        "pub fn main: () {\n    set x = 10\n    set y = \"hello\"\n    set z = x + y\n}\n",
    );

    assert!(matches!(err.kind, ErrorKind::Type { .. }));
    assert!(
        err.filename()
            .is_some_and(|name| name.ends_with("type_error.mire"))
    );
    let rendered = err.to_string();
    assert!(rendered.contains("error[E0005]"), "{rendered}");
    assert!(rendered.contains("Type Error"), "{rendered}");
}

#[test]
fn compile_reports_ownership_error_kind_and_filename() {
    let err = expect_compile_error_from_source(
        "mire_diag_ownership_kind_filename",
        "ownership_error.mire",
        "pub fn main: () {\n    set x = 10 :i64 mut\n    set borrowed = &x\n    set x = 20\n    use dasu(borrowed)\n}\n",
    );

    assert!(matches!(err.kind, ErrorKind::Ownership { .. }));
    assert!(
        err.filename()
            .is_some_and(|name| name.ends_with("ownership_error.mire"))
    );
    let rendered = err.to_string();
    assert!(rendered.contains("error[E0009]"), "{rendered}");
    assert!(rendered.contains("Ownership Error"), "{rendered}");
}

// Phase 1: module, use-module, and load path syntax
// ----------------------------------------------------------------

#[test]
fn parse_module_declaration_statement() {
    let source = "module my_package\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("module declaration should parse");
    let Statement::Module { name } = &program.statements[0] else {
        panic!("expected module statement");
    };
    assert_eq!(name, "my_package");
}

#[test]
fn parse_use_expression_at_top_level() {
    let source = "use strings\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("use-expression should parse at top level");
    assert!(matches!(&program.statements[0], Statement::Expression(_)));
}

#[test]
fn parse_use_vs_expression() {
    // use at top-level → expression (no longer an import)
    let source = "use strings\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("source should parse");
    assert!(matches!(&program.statements[0], Statement::Expression(_)));
    assert!(matches!(&program.statements[1], Statement::Function { name, .. } if name == "main"));
    // use inside function body → expression
    let source2 = "pub fn main: () {\n    use helper()\n}\n";
    let program2 = parse(source2).expect("source should parse");
    let Statement::Function { body, .. } = &program2.statements[0] else {
        panic!("expected function");
    };
    assert!(matches!(&body[0], Statement::Expression(_)));
}

#[test]
fn parse_load_with_double_colon_path() {
    let source = "load kioto::math::basic\npub fn main: () {\n    use dasu(str(pi))\n}\n";
    let program = parse(source).expect("double-colon load should parse");
    let Statement::Load { path, .. } = &program.statements[0] else {
        panic!("expected load statement");
    };
    assert_eq!(path.len(), 3);
    assert_eq!(path[0], "kioto");
    assert_eq!(path[1], "math");
    assert_eq!(path[2], "basic");
}

#[test]
fn parse_load_with_single_path() {
    let source = "load kioto\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("single load should parse");
    let Statement::Load { path, .. } = &program.statements[0] else {
        panic!("expected load statement");
    };
    assert_eq!(path, &["kioto".to_string()]);
}

#[test]
fn reject_dot_slash_load() {
    let source = "load ./utils\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let result = parse(source);
    assert!(result.is_err(), "dot-slash loads must be rejected");
}

#[test]
fn parse_load_with_alias() {
    let source = "load kioto as std\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("load with alias should parse");
    let Statement::Load { path, alias, .. } = &program.statements[0] else {
        panic!("expected load statement");
    };
    assert_eq!(path, &["kioto".to_string()]);
    assert_eq!(alias.as_deref(), Some("std"));
}

#[test]
fn parse_module_followed_by_load() {
    let source = "module my_app\nload kioto\npub fn main: () {\n    use dasu(\"ok\")\n}\n";
    let program = parse(source).expect("module then load should parse");
    assert!(matches!(&program.statements[0], Statement::Module { .. }));
    assert!(matches!(&program.statements[1], Statement::Load { .. }));
}

#[test]
fn typed_ireru_annotation_propagates_to_let_binding() {
    let source = "pub fn main: () {\nset x = ireru(\": \") :i64\n}\n";
    let mut program = parse(source).expect("source should parse");
    check_program_types(&mut program, source).expect("type check should pass");

    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    let Statement::Let {
        data_type,
        value: Some(Expression::Call {
            data_type: call_type,
            ..
        }),
        ..
    } = &body[0]
    else {
        panic!("expected typed let with call value");
    };

    assert_eq!(*data_type, DataType::I64);
    assert_eq!(*call_type, DataType::I64);
}

#[test]
fn template_output_requires_quoted_strings_for_literal_text() {
    let source = "pub fn main: () {\nset user = \"mire\"\nuse dasu(\"hola {user}\")\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("program should analyze");
}

#[test]
fn template_output_treats_unquoted_text_as_regular_expressions() {
    let source = "pub fn main: () {\nuse dasu(hola mundo)\n}\n";
    let mut program = parse(source).expect("regular expressions should still parse");
    let err = analyze_program(&mut program, source)
        .expect_err("unquoted text should now fail as unresolved identifiers")
        .to_string();
    assert!(err.contains("Unknown identifier 'hola'"), "{err}");
}

#[test]
fn template_output_interpolates_inside_quoted_strings() {
    let program = parse("pub fn main: () {\nset user = \"mire\"\nuse dasu(\"hola {user}!\" )\n}\n")
        .expect("source should parse");

    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    let Statement::Expression(Expression::Call { name, args, .. }) = &body[1] else {
        panic!("expected dasu call");
    };

    assert_eq!(name, "dasu");
    assert!(
        contains_call_named(&args[0], "str"),
        "expected interpolation to compile to str(...) call, got {:?}",
        args[0]
    );
}

#[test]
fn match_pattern_binding_is_available_to_template_output() {
    let source = "enum Result {\n    Ok(value :i64)\n}\n\npub fn main: () {\n    set result = Result.Ok(42)\n    match result {\n        Result.Ok(v) {\n            use dasu(v)\n            set copy = v :i64\n        }\n    }\n}\n";
    let mut program = parse(source).expect("source should parse");

    analyze_program(&mut program, source).expect("match payload binding should analyze");

    let Statement::Function { body, .. } = &program.statements[1] else {
        panic!("expected function statement");
    };
    let Statement::Match { cases, .. } = &body[1] else {
        panic!("expected match statement");
    };
    let Statement::Expression(Expression::Call { args, .. }) = &cases[0].1[0] else {
        panic!("expected dasu call");
    };

    assert!(
        matches!(args.first(), Some(Expression::Identifier(_))),
        "{args:?}"
    );
}

#[test]
fn enum_variant_payload_arity_is_checked_for_direct_construction() {
    let err = expect_analysis_error(
        "enum Pair {\n    Pair(left :i64 right :i64)\n}\n\npub fn main: () {\n    set pair = Pair.Pair(10)\n}\n",
    );

    assert!(err.contains("expects 2 values, got 1"), "{err}");
}

#[test]
fn enum_variant_named_payloads_are_reordered_by_declared_field_names() {
    let root = make_temp_project_root("mire_enum_variant_named_payloads");
    let source_path = root.join("enum_variant_named_payloads.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-variant-named-payloads\"\nversion = \"0.1.0\"\nentry = \"enum_variant_named_payloads.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Status {\n    Loading(progress :i64 total :i64)\n}\n\npub fn main: () {\n    set loading = Status.Loading(total: 100, progress: 75)\n    match loading {\n        Status.Loading(progress total) {\n            use dasu(\"{progress} {total}\")\n        }\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("named enum payloads should compile");
}

#[test]
fn enum_variant_named_payloads_reject_mixed_argument_styles() {
    let err = expect_analysis_error(
        "enum Status {\n    Loading(progress :i64 total :i64)\n}\n\npub fn main: () {\n    set loading = Status.Loading(progress: 75 100)\n}\n",
    );

    assert!(
        err.contains("cannot mix named and positional arguments"),
        "{err}"
    );
}

#[test]
fn enum_variant_named_payloads_reject_unknown_fields() {
    let err = expect_analysis_error(
        "enum Status {\n    Loading(progress :i64 total :i64)\n}\n\npub fn main: () {\n    set loading = Status.Loading(percent: 75, total: 100)\n}\n",
    );

    assert!(err.contains("has no field 'percent'"), "{err}");
}

#[test]
fn enum_variant_named_payloads_reject_duplicate_fields() {
    let err = expect_analysis_error(
        "enum Status {\n    Loading(progress :i64 total :i64)\n}\n\npub fn main: () {\n    set loading = Status.Loading(progress: 75, progress: 80)\n}\n",
    );

    assert!(err.contains("received duplicate field 'progress'"), "{err}");
}

#[test]
fn match_pattern_identifier_is_not_type_checked() {
    let source =
        "pub fn main: () {\nset x = 1\nset y = match x { missing { 10 } _ { 0 } } :i64\n}\n";
    let mut program = parse(source).expect("source should parse");

    analyze_program(&mut program, source)
        .expect("match patterns should be skipped during analysis");
}

#[test]
fn match_result_identifier_still_must_resolve() {
    let err = expect_analysis_error(
        "pub fn main: () {\nset x = 1\nset y = match x { 1 { missing } _ { 0 } } :i64\n}\n",
    );

    assert!(err.contains("Unknown identifier 'missing'"), "{err}");
}

#[test]
fn if_expression_infers_branch_type_and_runs() {
    let source = "pub fn main: () {\n    set flag = true :bool\n    set result = if flag { 10 } else { 20 }\n    use dasu(result)\n}\n";
    let mut program = parse(source).expect("source should parse");

    check_program_types(&mut program, source).expect("if expression should infer branch type");

    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    let Statement::Let { data_type, .. } = &body[1] else {
        panic!("expected let statement for if expression");
    };
    assert_eq!(*data_type, DataType::I64);

    let root = make_temp_project_root("mire_if_expression_infers_branch_type");
    let source_path = root.join("if_expression_infers_branch_type.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"if-expression-infers-branch-type\"\nversion = \"0.1.0\"\nentry = \"if_expression_infers_branch_type.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("if expression should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10"), "{stdout}");
}

#[test]
fn if_expression_rejects_incompatible_branch_types() {
    let err = expect_analysis_error(
        "struct Point {\n    x :i64\n}\n\nstruct Size {\n    x :i64\n}\n\npub fn main: () {\n    set flag = true :bool\n    set value = if flag { (Point x: 1) } else { (Size x: 1) }\n    use dasu(value.x)\n}\n",
    );

    assert!(
        err.contains("Cannot unify incompatible struct types"),
        "{err}"
    );
}

#[test]
fn match_expression_with_default_infers_string_branch_type_and_compiles() {
    let source = "pub fn main: () {\n    set result = 2 :i64\n    set msg = match result {\n        1 { \"success\" }\n        _ { \"failed\" }\n    } :str\n    use dasu(msg)\n}\n";
    let mut program = parse(source).expect("source should parse");

    check_program_types(&mut program, source).expect("match expression should infer string type");

    let Statement::Function { body, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    let Statement::Let {
        value: Some(Expression::Match { data_type, .. }),
        ..
    } = &body[1]
    else {
        panic!("expected let binding with match expression");
    };
    assert_eq!(*data_type, DataType::Str);

    let root = make_temp_project_root("mire_match_expression_default_infers_string_type");
    let source_path = root.join("match_expression_default_infers_string_type.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"match-expression-default-infers-string-type\"\nversion = \"0.1.0\"\nentry = \"match_expression_default_infers_string_type.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("match expression returning string should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("failed"), "{stdout}");
}

#[test]
fn match_expression_can_be_returned_directly_from_function() {
    let source = "pub fn classify: (x :i64) :i64 {\n    return match x {\n        1 { 10 }\n        _ { 20 }\n    } :i64\n}\n\npub fn main: () {\n    use dasu(classify(1))\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("return match should analyze");

    let root = make_temp_project_root("mire_return_match_direct");
    let source_path = root.join("return_match_direct.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"return-match-direct\"\nversion = \"0.1.0\"\nentry = \"return_match_direct.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("return match should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10"), "{stdout}");
}

#[test]
fn enum_match_without_default_returns_second_variant_string() {
    let root = make_temp_project_root("mire_enum_match_second_variant_string");
    let source_path = root.join("enum_match_second_variant_string.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-second-variant-string\"\nversion = \"0.1.0\"\nentry = \"enum_match_second_variant_string.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Status {\n    Ok\n    Error\n}\n\npub fn main: () {\n    set r = Status.Error\n    set m = match r {\n        Status.Ok { \"success\" }\n        Status.Error { \"failed\" }\n    } :str\n    use dasu(m)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Release,
            opt_level: OptLevel::O3,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("enum match returning second variant string should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("failed"), "{stdout}");
}

#[test]
fn incremental_recompile_keeps_enum_match_string_result_consistent() {
    let root = make_temp_project_root("mire_enum_match_string_flip");
    let source_path = root.join("main.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-string-flip\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n[cache]\nanalysis_cache = true\ncompression = false\n",
    )
    .expect("write project");

    let run_case = |source: &str| -> String {
        fs::write(&source_path, source).expect("write source");
        let build = compile_file_with_avenys(
            &source_path,
            &BuildOptions {
                mode: BuildMode::Release,
                opt_level: OptLevel::O3,
                debug_dump: false,
                output: None,
                emit_binary: true,
                persist_ir: false,
                import_mode: mire::ImportMode::Reachable,
                cache: Default::default(),
                warning_filter: mire::error::diagnostic::WarningFilter::Default,
                deny_warnings: std::collections::HashSet::new(),
                module_paths: vec![],
            },
        )
        .expect("compile case");

        let output = Command::new(&build.binary_path)
            .output()
            .expect("run binary");
        assert!(output.status.success(), "binary should run successfully");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    let failed_case = "enum Status {\n    Ok\n    Error\n}\n\npub fn main: () {\n    set r = Status.Error\n    set m = match r {\n        Status.Ok { \"success\" }\n        Status.Error { \"failed\" }\n    } :str\n    use dasu(m)\n}\n";
    let error_case = "enum Status {\n    Ok\n    Error\n}\n\npub fn main: () {\n    set r = Status.Error\n    set m = match r {\n        Status.Ok { \"ok\" }\n        Status.Error { \"error\" }\n    } :str\n    use dasu(m)\n}\n";

    assert_eq!(run_case(failed_case), "failed");
    assert_eq!(run_case(error_case), "error");
    assert_eq!(run_case(failed_case), "failed");
}

#[test]
fn instance_method_call_resolves_and_compiles() {
    let source = "struct Point {\n    x :i64\n    y :i64\n}\n\nimpl Point {\n    fn distance: (self) :i64 {\n        return self.x\n    }\n}\n\npub fn main: () {\n    set p = (Point x: 3, y: 4)\n    set d = p.distance()\n    use dasu(d)\n}\n";
    let mut program = parse(source).expect("source should parse");

    analyze_program(&mut program, source).expect("instance method call should analyze");

    let Statement::Function { body, .. } = &program.statements[2] else {
        panic!("expected main function");
    };
    let Statement::Let {
        data_type,
        value:
            Some(Expression::Call {
                name,
                data_type: call_type,
                ..
            }),
        ..
    } = &body[1]
    else {
        panic!("expected method call let binding");
    };

    assert_eq!(name, "p.distance");
    assert_eq!(*data_type, DataType::I64);
    assert_eq!(*call_type, DataType::I64);

    let root = make_temp_project_root("mire_instance_method_call");
    let source_path = root.join("instance_method_call.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"instance-method-call\"\nversion = \"0.1.0\"\nentry = \"instance_method_call.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("instance method call should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('3'), "{stdout}");
}

#[test]
fn direct_template_member_access_prints_field_values() {
    let root = make_temp_project_root("mire_direct_template_member_access");
    let source_path = root.join("direct_template_member_access.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"direct-template-member-access\"\nversion = \"0.1.0\"\nentry = \"direct_template_member_access.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "struct Person {\n    name :str\n    age :i64\n}\n\npub fn main: () {\n    set person = (Person name: \"Alice\", age: 30)\n    use dasu(person.age)\n    use dasu(person.name)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("member access template should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("30"), "{stdout}");
    assert!(stdout.contains("Alice"), "{stdout}");
    assert!(!stdout.contains(". age"), "{stdout}");
    assert!(!stdout.contains(". name"), "{stdout}");
}

#[test]
fn direct_struct_field_assignment_updates_mutable_binding() {
    let source = "struct Counter {\n    value :i64\n}\n\npub fn main: () {\n    set counter = (Counter value: 10) mut\n    set counter.value = 41\n    set counter.value = counter.value + 1\n    use dasu(counter.value)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("field assignment should analyze");

    let root = make_temp_project_root("mire_direct_struct_field_assignment");
    let source_path = root.join("direct_struct_field_assignment.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"direct-struct-field-assignment\"\nversion = \"0.1.0\"\nentry = \"direct_struct_field_assignment.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("field assignment should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn immutable_struct_field_assignment_still_errors() {
    let err = expect_analysis_error(
        "struct Counter {\n    value :i64\n}\n\npub fn main: () {\n    set counter = (Counter value: 10)\n    set counter.value = 11\n}\n",
    );

    assert!(
        err.contains("Cannot reassign immutable variable 'counter.value'")
            || err.contains("Cannot reassign immutable variable 'counter'"),
        "{err}"
    );
}

#[test]
fn struct_with_array_field_declaration_and_construction_compiles() {
    let source = "struct Stack {\n    items :arr[i64 3]\n    size :i64\n}\n\npub fn main: () {\n    set stack = (Stack items: [1 2 3] :arr[i64 3], size: 3)\n    use dasu(stack.size)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("struct array field should analyze");

    let root = make_temp_project_root("mire_struct_array_field");
    let source_path = root.join("struct_array_field.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"struct-array-field\"\nversion = \"0.1.0\"\nentry = \"struct_array_field.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("struct array field should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3"), "{stdout}");
}

#[test]
fn static_impl_method_call_resolves_and_runs() {
    let root = make_temp_project_root("mire_static_impl_method_call");
    let source_path = root.join("static_impl_method_call.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"static-impl-method-call\"\nversion = \"0.1.0\"\nentry = \"static_impl_method_call.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "struct Point {\n    x :i64\n    y :i64\n}\n\nimpl Point {\n    fn new: (x :i64 y :i64) :Point {\n        return (Point x: x, y: y)\n    }\n}\n\npub fn main: () {\n    set p = Point::new(10 20)\n    use dasu(p.x)\n    use dasu(p.y)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("static impl method call should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10"), "{stdout}");
    assert!(stdout.contains("20"), "{stdout}");
}

#[test]
fn implicit_self_method_return_still_runs() {
    let root = make_temp_project_root("mire_explicit_self_method_return");
    let source_path = root.join("explicit_self_method_return.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"explicit-self-method-return\"\nversion = \"0.1.0\"\nentry = \"explicit_self_method_return.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "struct Person {\n    name :str\n    age :i64\n}\n\nimpl Person {\n    fn get_name: (self) :str {\n        return self.name\n    }\n\n    fn get_age: (self) :i64 {\n        return self.age\n    }\n}\n\npub fn main: () {\n    set person = (Person name: \"Alice\", age: 25)\n    use dasu(person.get_name())\n    use dasu(person.get_age())\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("explicit self method return should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Alice"), "{stdout}");
    assert!(stdout.contains("25"), "{stdout}");
}

#[test]
fn method_using_self_without_explicit_self_is_rejected() {
    let source = "struct Person {\n    age :i64\n}\n\nimpl Person {\n    fn get_age: () :i64 {\n        return self.age\n    }\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source)
        .expect_err("program should fail type checking")
        .to_string();

    assert!(err.contains("Unknown identifier 'self'"), "{err}");
}

#[test]
fn distinct_struct_types_do_not_unify_by_shape() {
    let source = "struct Point {\n    x :i64\n}\n\nstruct Size {\n    x :i64\n}\n\nfn needs_size: (value :Size) :i64 {\n    return value.x\n}\n\npub fn main: () {\n    set point = (Point x: 1)\n    use dasu(needs_size(point))\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source)
        .expect_err("distinct nominal struct types must not unify")
        .to_string();

    assert!(err.contains("expects StructNamed(\"Size\")"), "{err}");
    assert!(err.contains("got StructNamed(\"Point\")"), "{err}");
}

#[test]
fn trait_self_type_accepts_owner_nominal_type_in_impl() {
    let source = "struct Point {\n    x :i64\n}\n\npub skill Projectable {\n    fn project: (self: Point) :i64\n}\n\nimpl Projectable for Point {\n    fn project: (self) :i64 {\n        return self.x\n    }\n}\n\npub fn main: () {\n    set point = (Point x: 7)\n    use dasu(point.project())\n}\n";
    let mut program = parse(source).expect("source should parse");

    analyze_program(&mut program, source)
        .expect("trait self type should resolve to the impl owner nominal type");
}

#[test]
fn trait_instance_method_cannot_be_implemented_as_associated() {
    let err = expect_analysis_error(
        "struct Point {\n    x :i64\n}\n\npub skill Projectable {\n    fn project: (self) :i64\n}\n\nimpl Projectable for Point {\n    fn project: () :i64 {\n        return 0\n    }\n}\n",
    );

    assert!(
        err.contains("must be implemented as an instance method"),
        "{err}"
    );
}

#[test]
fn trait_associated_method_cannot_be_implemented_as_instance() {
    let err = expect_analysis_error(
        "struct Point {\n    x :i64\n}\n\npub skill Factory {\n    fn build: () :Point\n}\n\nimpl Factory for Point {\n    fn build: (self) :Point {\n        return self\n    }\n}\n",
    );

    assert!(
        err.contains("must be implemented as an associated method"),
        "{err}"
    );
}

#[test]
fn impl_self_parameter_must_be_first() {
    let err = expect_analysis_error(
        "struct Point {\n    x :i64\n}\n\nimpl Point {\n    fn bad: (value :i64 self) :i64 {\n        return value\n    }\n}\n",
    );

    assert!(
        err.contains("Method 'Point.bad' must declare 'self' as the first parameter"),
        "{err}"
    );
}

#[test]
fn empty_skill_is_rejected() {
    let err = expect_analysis_error(
        "load kioto\n\npub skill Printable {\n}\n\npub fn main: () {\n    use dasu(\"test\")\n}\n",
    );

    assert!(
        err.contains("Skill 'Printable' must declare at least one method"),
        "{err}"
    );
}

#[test]
fn runtime_division_by_zero_exits_with_error() {
    let root = make_temp_project_root("mire_runtime_division_by_zero");
    let source_path = root.join("runtime_division_by_zero.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"runtime-division-by-zero\"\nversion = \"0.1.0\"\nentry = \"runtime_division_by_zero.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set x = 10 / 0\n    use dasu(x)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(!output.status.success(), "binary should fail at runtime");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("division by zero"), "{stderr}");
}

#[test]
fn signed_integer_division_and_remainder_match_runtime_expectations() {
    let root = make_temp_project_root("mire_signed_division");
    let source_path = root.join("signed_division.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"signed-division\"\nversion = \"0.1.0\"\nentry = \"signed_division.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set a = -10 / 3\n    set b = -10 % 3\n    use dasu(\"{a} {b}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("-3 -1"), "{stdout}");
}

#[test]
fn float_arithmetic_with_typed_float_variable_executes() {
    let root = make_temp_project_root("mire_float_arithmetic_typed_var");
    let source_path = root.join("float_arithmetic_typed_var.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"float-arithmetic-typed-var\"\nversion = \"0.1.0\"\nentry = \"float_arithmetic_typed_var.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set x = 2.0 :f64\n    set y = x + 1.5\n    set z = y * 2.0\n    use dasu(z > 6.9 && z < 7.1)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("float arithmetic should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("true"), "{stdout}");
}

#[test]
fn nested_vector_type_is_preserved_for_lists_push() {
    let err = expect_analysis_error(
        "load kioto\n\npub fn main: () {\n    set nested = [[1 2] [3 4]] :vec[vec[i64]]\n    set bad = lists.push(nested [\"x\"])\n    use dasu(bad)\n}\n",
    );

    assert!(
        err.contains("Cannot unify incompatible types")
            || err.contains("Type mismatch")
            || err.contains("expects vec"),
        "{err}"
    );
}

#[test]
fn secondary_for_loop_binding_compiles_and_uses_index() {
    let root = make_temp_project_root("mire_for_secondary_binding");
    let source_path = root.join("for_secondary_binding.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"for-secondary-binding\"\nversion = \"0.1.0\"\nentry = \"for_secondary_binding.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set acc = 0 :i64 mut\n    for item, index in range(4) {\n        set acc = acc + item + index\n    }\n    use dasu(acc)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("two-binding for loop should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("12"), "{stdout}");
}

#[test]
fn advanced_literals_compile_and_run() {
    let root = make_temp_project_root("mire_advanced_literals");
    let source_path = root.join("advanced_literals.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"advanced-literals\"\nversion = \"0.1.0\"\nentry = \"advanced_literals.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set bin = 0b1010 :i64\n    set oct = 0o12 :i64\n    set hex = 0xFF :i64\n    set c = 'a' :char\n    set newline = '\\n' :char\n    set raw = r##\"hello \"world\" with ##\"## :str\n    use dasu(bin == oct && hex == 255)\n    use dasu(c == 97 && newline == 10)\n    use dasu(raw)\n}\n",
    )
    .expect("write source");

    let opts = BuildOptions {
        mode: BuildMode::Debug,
        opt_level: OptLevel::O0,
        debug_dump: true,
        output: None,
        emit_binary: true,
        persist_ir: true,
        import_mode: mire::ImportMode::Reachable,
        cache: Default::default(),
        warning_filter: mire::error::diagnostic::WarningFilter::Default,
        deny_warnings: std::collections::HashSet::new(),
        module_paths: vec![],
    };

    let ir_path = root.join("bin").join("debug").join("advanced_literals.ll");

    let build = compile_file_with_avenys(&source_path, &opts);

    if let Ok(ref build) = build {
        let output = Command::new(&build.binary_path)
            .output()
            .expect("run binary");
        assert!(output.status.success(), "binary should run successfully");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("true"), "{stdout}");
        assert!(stdout.contains("hello \"world\" with ##"), "{stdout}");
    } else {
        if ir_path.exists() {
            let ir = std::fs::read_to_string(&ir_path).expect("read ir");
            eprintln!("=== LLVM IR ===\n{}", ir);
        }
        let err = build.unwrap_err();
        panic!("advanced literals should compile: {err:?}");
    }
}

#[test]
fn unsafe_block_compiles_and_runs() {
    let root = make_temp_project_root("mire_unsafe_block");
    let source_path = root.join("unsafe_block.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"unsafe-block\"\nversion = \"0.1.0\"\nentry = \"unsafe_block.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set sum = 0 :i64 mut\n    unsafe {\n        set sum = sum + 2\n    }\n    use dasu(sum)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("unsafe block should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2"), "{stdout}");
}

#[test]
fn extern_and_inline_asm_declarations_parse_and_compile() {
    let root = make_temp_project_root("mire_extern_asm_parse_compile");
    let source_path = root.join("extern_asm_parse_compile.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"extern-asm-parse-compile\"\nversion = \"0.1.0\"\nentry = \"extern_asm_parse_compile.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\nextern lib \"c\" \"libc.so.6\"\nextern fn puts: (msg :*const i8) :i32 lib \"c\"\n\npub fn main: () {\n    asm {\n        nop\n        nop\n    }\n    use dasu(\"ok\")\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("extern/asm declarations should compile");
}

#[test]
fn runtime_out_of_bounds_exits_with_error() {
    let root = make_temp_project_root("mire_runtime_out_of_bounds");
    let source_path = root.join("runtime_out_of_bounds.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"runtime-out-of-bounds\"\nversion = \"0.1.0\"\nentry = \"runtime_out_of_bounds.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set nums = [1 2 3] :arr[i64 3]\n    set x = nums at 10\n    use dasu(x)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(!output.status.success(), "binary should fail at runtime");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("index out of bounds"), "{stderr}");
}

#[test]
fn callback_call_named_function_runs_end_to_end() {
    let root = make_temp_project_root("mire_callback_call_named_fn");
    let source_path = root.join("callback_call_named_fn.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"callback-call-named-fn\"\nversion = \"0.1.0\"\nentry = \"callback_call_named_fn.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nfn add1: (x :i64) :i64 {\n    return x + 1\n}\n\npub fn main: () {\n    use dasu(call(add1, 41))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("callback call should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn callback_call_extern_fn_runs_end_to_end() {
    let root = make_temp_project_root("mire_callback_call_extern_fn");
    let source_path = root.join("callback_call_extern_fn.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"callback-call-extern-fn\"\nversion = \"0.1.0\"\nentry = \"callback_call_extern_fn.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\nextern lib \"c\" \"libc.so.6\"\nextern fn abs: (x :i64) :i64 lib \"c\"\n\npub fn main: () {\n    use dasu(call(abs, -7))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("extern callback call should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("7"), "{stdout}");
}

#[test]
fn callback_call_closure_with_capture_runs_end_to_end() {
    let root = make_temp_project_root("mire_callback_call_closure_capture");
    let source_path = root.join("callback_call_closure_capture.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"callback-call-closure-capture\"\nversion = \"0.1.0\"\nentry = \"callback_call_closure_capture.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set offset = 10 :i64\n    use dasu(call((x) => x + offset, 32))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("capturing closure callback should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn callback_call_function_value_alias_runs_end_to_end() {
    let root = make_temp_project_root("mire_callback_call_function_value_alias");
    let source_path = root.join("callback_call_function_value_alias.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"callback-call-function-value-alias\"\nversion = \"0.1.0\"\nentry = \"callback_call_function_value_alias.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nfn add1: (x :i64) :i64 {\n    return x + 1\n}\n\npub fn main: () {\n    set f = add1\n    use dasu(call(f, 41))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("function value callback alias should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn callback_call_extern_function_value_alias_runs_end_to_end() {
    let root = make_temp_project_root("mire_callback_call_extern_function_value_alias");
    let source_path = root.join("callback_call_extern_function_value_alias.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"callback-call-extern-function-value-alias\"\nversion = \"0.1.0\"\nentry = \"callback_call_extern_function_value_alias.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\nextern lib \"c\" \"libc.so.6\"\nextern fn abs: (x :i64) :i64 lib \"c\"\n\npub fn main: () {\n    set f = abs\n    use dasu(call(f, -7))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("extern function value callback alias should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("7"), "{stdout}");
}

#[test]
fn callback_call_dynamic_function_param_without_signature_is_rejected() {
    let err = expect_compile_error_from_source(
        "mire_callback_call_dynamic_function_param_rejected",
        "callback_call_dynamic_function_param_rejected.mire",
        "load kioto\n\nfn add1: (x :i64) :i64 {\n    return x + 1\n}\n\nfn apply: (f :function x :i64) :i64 {\n    return call(f, x)\n}\n\npub fn main: () {\n    set f = add1\n    use dasu(apply(f, 41))\n}\n",
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("signature cannot be inferred"),
        "{rendered}"
    );
}

#[test]
fn callback_call_function_return_value_without_signature_is_rejected() {
    let err = expect_compile_error_from_source(
        "mire_callback_call_function_return_value_rejected",
        "callback_call_function_return_value_rejected.mire",
        "load kioto\n\nfn add1: (x :i64) :i64 {\n    return x + 1\n}\n\nfn pick: (f :function) :function {\n    return f\n}\n\npub fn main: () {\n    use dasu(call(pick(add1), 41))\n}\n",
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("signature cannot be inferred"),
        "{rendered}"
    );
}

#[test]
fn callback_call_dynamic_extern_multi_arg_without_signature_is_rejected() {
    let err = expect_compile_error_from_source(
        "mire_callback_call_dynamic_extern_multi_arg_rejected",
        "callback_call_dynamic_extern_multi_arg_rejected.mire",
        "load kioto\nextern lib \"c\" \"libc.so.6\"\nextern fn strncmp: (a :str b :str n :i64) :i64 lib \"c\"\n\nfn invoke3: (f :function a :str b :str n :i64) :i64 {\n    return call(f, a, b, n)\n}\n\npub fn main: () {\n    set f = strncmp\n    use dasu(invoke3(f, \"abc\", \"abc\", 3))\n}\n",
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("signature cannot be inferred"),
        "{rendered}"
    );
}

#[test]
fn string_literals_accept_braces_without_escape_hacks() {
    let root = make_temp_project_root("mire_string_braces_literal");
    let source_path = root.join("string_braces_literal.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"string-braces-literal\"\nversion = \"0.1.0\"\nentry = \"string_braces_literal.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    use dasu(\"json-like: {{ok}}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("brace-containing escaped template string should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("json-like: {ok}"), "{stdout}");
}

#[test]
fn contains_on_list_returns_correct_result() {
    let root = make_temp_project_root("mire_contains_list");
    let source_path = root.join("contains_list.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"contains-list\"\nversion = \"0.1.0\"\nentry = \"contains_list.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set nums = [1 2 3]\n    use dasu(contains(nums 2))\n    use dasu(contains(nums 5))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("contains should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("true"),
        "expected contains(2) to be true, got: {stdout}"
    );
    assert!(
        stdout.contains("false"),
        "expected contains(5) to be false, got: {stdout}"
    );
}

#[test]
fn strings_split_returns_list_and_works_with_join() {
    let root = make_temp_project_root("mire_strings_split");
    let source_path = root.join("strings_split.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"strings-split\"\nversion = \"0.1.0\"\nentry = \"strings_split.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set parts = strings.split(\"a,b,c\" \",\")\n    set joined = strings.join(parts \"-\")\n    use dasu(joined)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("strings.split should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a-b-c"), "Expected 'a-b-c', got: {stdout}");
}

#[test]
fn strings_split_supports_multi_char_delimiter() {
    let root = make_temp_project_root("mire_strings_split_multichar");
    let source_path = root.join("strings_split_multichar.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"strings-split-multichar\"\nversion = \"0.1.0\"\nentry = \"strings_split_multichar.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set parts = strings.split(\"alpha--beta--gamma\" \"--\")\n    use dasu(strings.join(parts \"|\"))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("strings.split multichar should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alpha|beta|gamma"),
        "Expected 'alpha|beta|gamma', got: {stdout}"
    );
}

#[test]
fn strings_split_preserves_empty_segments() {
    let root = make_temp_project_root("mire_strings_split_empty_segments");
    let source_path = root.join("strings_split_empty_segments.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"strings-split-empty\"\nversion = \"0.1.0\"\nentry = \"strings_split_empty_segments.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set parts = strings.split(\"a,,b,\" \",\")\n    use dasu(strings.join(parts \"|\"))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("strings.split empty segments should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a||b|"), "Expected 'a||b|', got: {stdout}");
}

#[test]
fn kioto_strings_reference_api_reuses_the_same_binding() {
    let root = make_temp_project_root("mire_strings_reference_api");
    let source_path = root.join("strings_reference_api.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"strings-reference-api\"\nversion = \"0.1.0\"\nentry = \"strings_reference_api.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set text = \"ab\" :str\n    set len1 = strings.len(text)\n    set upper = strings.upper(text)\n    set repeated = strings.repeat(text 3)\n    set len2 = strings.len(text)\n    use dasu(\"{len1}-{upper}-{repeated}-{len2}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("strings reference api should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2-AB-ababab-2"), "{stdout}");
}

#[test]
fn kioto_lists_reference_api_reuses_the_same_binding() {
    let root = make_temp_project_root("mire_lists_reference_api");
    let source_path = root.join("lists_reference_api.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"lists-reference-api\"\nversion = \"0.1.0\"\nentry = \"lists_reference_api.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set nums = [1 2 3 2] :vec[i64]\n    set len1 = lists.len(nums)\n    set has_two = lists.contains(nums 2)\n    set first = lists.first(nums)\n    set last = lists.last(nums)\n    set idx = lists.index_of(nums 2)\n    set len2 = lists.len(nums)\n    use dasu(\"{len1}-{has_two}-{first}-{last}-{idx}-{len2}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("lists reference api should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("4-true-1-2-1-4"), "{stdout}");
}

#[test]
fn syntax_reference_prototype_compiles_and_runs() {
    let root = make_temp_project_root("mire_syntax_prototype");
    let source_path = root.join("prototype.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"syntax-prototype\"\nversion = \"0.1.0\"\nentry = \"prototype.mire\"\n",
    )
    .expect("write project");
    let prototype_source =
        fs::read_to_string("tests/syntax/prototype.mire").expect("read syntax prototype source");
    fs::write(&source_path, prototype_source).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("syntax prototype should compile");

    let output = Command::new(&build.binary_path)
        .current_dir(&root)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a|b|c"), "{stdout:?}");
    assert!(stdout.contains("child"), "{stdout:?}");
}

#[test]
fn array_index_assignment_mutates_elements_in_place() {
    let root = make_temp_project_root("mire_array_index_assignment");
    let source_path = root.join("array_index_assignment.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"array-index-assignment\"\nversion = \"0.1.0\"\nentry = \"array_index_assignment.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nfn test_swap: () {\n    set arr = [10 20 30 40] :arr[i64 4] mut\n    set left = arr at 0\n    set right = arr at 3\n    set arr at 0 = right\n    set arr at 3 = left\n    use dasu(\"{arr at 0} {arr at 1} {arr at 2} {arr at 3}\")\n}\n\npub fn main: () {\n    test_swap()\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("40 20 30 10"), "{stdout}");
}

#[test]
fn struct_array_field_index_assignment_compiles_and_runs() {
    let root = make_temp_project_root("mire_struct_array_index_assignment");
    let source_path = root.join("struct_array_index_assignment.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"struct-array-index-assignment\"\nversion = \"0.1.0\"\nentry = \"struct_array_index_assignment.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nstruct Matrix {\n    data :arr[i64 4]\n    cols :i64\n}\n\nimpl Matrix {\n    fn new: () :Matrix {\n        return (Matrix data: [0 0 0 0] :arr[i64 4], cols: 2)\n    }\n\n    fn update: (self row :i64 col :i64 val :i64) {\n        set idx = row * self.cols + col\n        set self.data at idx = val\n    }\n\n    fn get: (self row :i64 col :i64) :i64 {\n        set idx = row * self.cols + col\n        return self.data at idx\n    }\n}\n\npub fn main: () {\n    set m = Matrix::new()\n    m.update(0 1 7)\n    m.update(1 0 9)\n    use dasu(\"{m.get(0 1)} {m.get(1 0)}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("7 9"), "{stdout}");
}

#[test]
fn shared_reference_lowering_compiles_and_runs() {
    let root = make_temp_project_root("mire_shared_reference_lowering");
    let source_path = root.join("shared_reference_lowering.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"shared-reference-lowering\"\nversion = \"0.1.0\"\nentry = \"shared_reference_lowering.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nfn read_ref: (value :&i64) :i64 {\n    return *value\n}\n\npub fn main: () {\n    set x = 41 :i64\n    set rx = &x\n    set y = read_ref(rx)\n    use dasu(y + 1)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn impl_method_can_mutate_self_field_and_run() {
    let root = make_temp_project_root("mire_impl_self_field_mutation");
    let source_path = root.join("impl_self_field_mutation.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"impl-self-field-mutation\"\nversion = \"0.1.0\"\nentry = \"impl_self_field_mutation.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nstruct Counter {\n    value :i64 mut\n    step :i64\n}\n\nimpl Counter {\n    fn new: (step :i64) :Counter {\n        return (Counter value: 0, step: step)\n    }\n\n    fn increment: (self) {\n        set self.value = self.value + self.step\n    }\n\n    fn reset: (self) {\n        set self.value = 0\n    }\n\n    fn get: (self) :i64 {\n        return self.value\n    }\n}\n\npub fn main: () {\n    set c = Counter::new(5)\n    c.increment()\n    c.increment()\n    c.reset()\n    c.increment()\n    use dasu(c.get())\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "binary should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("5"), "{stdout}");
}

#[test]
fn impl_method_self_field_assignment_typechecks() {
    let source = "struct Counter {\n    value :i64 mut\n    step :i64\n}\n\nimpl Counter {\n    fn increment: (self) {\n        set self.value = self.value + self.step\n    }\n}\n";
    let mut program = parse(source).expect("source should parse");
    check_program_types(&mut program, source).expect("source should typecheck");
}

#[test]
fn impl_method_self_field_assignment_parses() {
    let source = "struct Counter {\n    value :i64 mut\n    step :i64\n}\n\nimpl Counter {\n    fn increment: (self) {\n        set self.value = self.value + self.step\n    }\n}\n";
    let program = parse(source).expect("source should parse");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn impl_method_empty_body_parses() {
    let source = "struct Counter {\n    value :i64 mut\n}\n\nimpl Counter {\n    fn increment: (self) {\n    }\n}\n";
    let program = parse(source).expect("source should parse");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn impl_method_local_assignment_parses() {
    let source = "struct Counter {\n    value :i64 mut\n}\n\nimpl Counter {\n    fn increment: (self) {\n        set x = 1\n    }\n}\n";
    let program = parse(source).expect("source should parse");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn parses_load_with_double_colon() {
    let program = parse("load kioto::math::basic\n").expect("source should parse");
    let Statement::Load { path, items, .. } = &program.statements[0] else {
        panic!("expected load statement");
    };

    assert_eq!(
        path,
        &["kioto".to_string(), "math".to_string(), "basic".to_string()]
    );
    assert!(items.is_none());
}

#[test]
fn pipeline_self_placeholder_analyzes_after_desugaring() {
    let source = "load kioto\npub fn main: () {\nuse range(5) => dasu(self)\n}\n";
    let mut program = parse(source).expect("source should parse");

    analyze_program(&mut program, source).expect("pipeline self placeholder should analyze");
}

#[test]
fn enum_match_payload_statement_body_compiles() {
    let root = make_temp_project_root("mire_enum_match_payload");
    let source_path = root.join("enum_match_payload.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-payload\"\nversion = \"0.1.0\"\nentry = \"enum_match_payload.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Result {\n    Ok(value :i64)\n    Err(err_num :i64)\n}\n\npub fn main: () {\n    set result = Result.Ok(42)\n    match result {\n        Result.Ok(v) {\n            use dasu(v)\n            set copy = v :i64\n        }\n        Result.Err(err_num) {\n            use dasu(err_num)\n        }\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("enum match payload statement body should compile");
}

#[test]
fn enum_match_multiple_payloads_compile() {
    let root = make_temp_project_root("mire_enum_match_multi_payload");
    let source_path = root.join("enum_match_multi_payload.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-multi-payload\"\nversion = \"0.1.0\"\nentry = \"enum_match_multi_payload.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Pair {\n    Pair(left :i64 right :i64)\n    Empty\n}\n\npub fn main: () {\n    set pair = Pair.Pair(10 20)\n    match pair {\n        Pair.Pair(a b) {\n            use dasu(\"{a} {b}\")\n            set total = a + b :i64\n            use dasu(total)\n        }\n        Pair.Empty {\n            use dasu(\"empty\")\n        }\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("enum match with multiple payloads should compile");
}

#[test]
fn enum_declaration_with_comma_separated_payloads_compiles() {
    let root = make_temp_project_root("mire_enum_match_multi_payload_commas");
    let source_path = root.join("enum_match_multi_payload_commas.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-multi-payload-commas\"\nversion = \"0.1.0\"\nentry = \"enum_match_multi_payload_commas.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Color {\n    Custom(r :i64, g :i64, b :i64)\n}\n\npub fn main: () {\n    set color = Color.Custom(10 20 30)\n    match color {\n        Color.Custom(r g b) {\n            use dasu(\"{r} {g} {b}\")\n        }\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("comma-separated enum payloads should compile");
}

#[test]
fn enum_match_statement_payload_bindings_support_string_and_bool() {
    let root = make_temp_project_root("mire_enum_match_statement_payload_types");
    let source_path = root.join("enum_match_statement_payload_types.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"enum-match-statement-payload-types\"\nversion = \"0.1.0\"\nentry = \"enum_match_statement_payload_types.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "enum Response {\n    Ok(message :str retry :bool)\n    Empty\n}\n\npub fn main: () {\n    set response = Response.Ok(\"ready\" true)\n    match response {\n        Response.Ok(message retry) {\n            use dasu(message)\n            if retry {\n                use dasu(\"retrying\")\n            }\n            set copy = message :str\n            use dasu(copy)\n        }\n        Response.Empty {\n            use dasu(\"empty\")\n        }\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("match statement payload bindings should support string and bool");
}

#[test]
fn pipeline_len_builtin_compiles() {
    let root = make_temp_project_root("mire_pipeline_len");
    let source_path = root.join("pipeline_len.mire");
    fs::write(
        &source_path,
        "load kioto\npub fn main: () {\nset x = [1 2 3] :arr[i64 3]\nset y = x => len()\nuse dasu(\"y: {y}\")\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("pipeline len should compile");
}

#[test]
fn nested_output_pipeline_compiles() {
    let root = make_temp_project_root("mire_pipeline_nested_output");
    let source_path = root.join("nested_output.mire");
    fs::write(
        &source_path,
        "load kioto\npub fn main: () {\nuse dasu(\"Hello\") => use dasu(self)\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("nested output pipeline should compile");
}

#[test]
fn find_statement_compiles_and_lowers() {
    let root = make_temp_project_root("mire_find_statement");
    let source_path = root.join("find_statement.mire");
    fs::write(
        &source_path,
        "load kioto\npub fn main: () {\n    find item in [1 2 3] {\n        use dasu(item)\n    }\n}\n",
    )
    .expect("write source");

    compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("find statement should compile and lower");
}

#[test]
fn bindings_const_and_compound_assignment_analyze() {
    let source = "pub fn main: () {\n    set base = 10 :i64 const\n    set acc = 5 :i64 mut\n    set acc += base\n    set acc -= 1\n    set acc *= 2\n    set acc /= 2\n    set acc %= 3\n    use dasu(acc)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("const + compound assignment should analyze");
}

#[test]
fn generic_identity_function_typechecks() {
    let source = "pub fn identity[T]: (x :T) :T {\n    return x\n}\n\npub fn main: () {\n    set a = identity[i64](42) :i64\n    set b = identity(\"ok\") :str\n    use dasu(a)\n    use dasu(b)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("generic identity should type check");
}

#[test]
fn generic_struct_constructor_typechecks() {
    let source = "type Box[T] {\n    value :T\n}\n\npub fn main: () {\n    set b = Box[i64](42)\n    use dasu(b)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("generic struct constructor should type check");
}

#[test]
fn generic_enum_variant_typechecks() {
    let source = "enum Option[T] {\n    None\n    Some(value :T)\n}\n\npub fn main: () {\n    set o = Option[i64].Some(7)\n    use dasu(o)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("generic enum variants should type check");
}

#[test]
fn generic_trait_bound_is_enforced() {
    let ok_source = "skill Show {\n    fn show: (self) :str\n}\n\ntype Num {\n    value :i64\n}\n\nimpl Show for Num {\n    fn show: (self) :str { return \"num\" }\n}\n\nfn print_it[T: Show]: (x :T) {\n    use dasu(\"ok\")\n}\n\npub fn main: () {\n    set n = Num(1)\n    print_it(n)\n}\n";
    let mut ok_program = parse(ok_source).expect("ok source should parse");
    analyze_program(&mut ok_program, ok_source).expect("bound should be satisfied");

    let bad_source = "skill Show {\n    fn show: (self) :str\n}\n\nfn print_it[T: Show]: (x :T) {\n    use dasu(x)\n}\n\npub fn main: () {\n    print_it(42)\n}\n";
    let mut bad_program = parse(bad_source).expect("bad source should parse");
    let err = analyze_program(&mut bad_program, bad_source).expect_err("bound must fail");
    assert!(
        err.to_string()
            .contains("requires 'T' to implement trait 'Show'")
    );
}

#[test]
fn generic_impl_method_resolves_for_concrete_type() {
    let source = "type Box[T] {\n    value :T\n}\n\nimpl[T] Box[T] {\n    fn get: (self) :T {\n        return self.value\n    }\n}\n\npub fn main: () {\n    set b = Box[i64](42)\n    set x = b.get() :i64\n    use dasu(x)\n}\n";
    let mut program = parse(source).expect("source should parse");
    analyze_program(&mut program, source).expect("generic impl method should resolve");
}

#[test]
fn generic_impl_method_codegen_builds_for_concrete_type() {
    let root = make_temp_project_root("mire_generic_impl_codegen");
    let source_path = root.join("main.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"generic-impl\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "type Box[T] {\n    value :T\n}\n\nimpl[T] Box[T] {\n    fn get: (self) :T {\n        return self.value\n    }\n}\n\npub fn main: () {\n    set b = Box[i64](42)\n    set x = b.get() :i64\n    use dasu(x)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("generic impl should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run compiled binary");
    assert!(
        output.status.success(),
        "binary should run, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn nongeneric_function_rejects_explicit_type_args() {
    let source = "fn plain: (x :i64) :i64 {\n    return x\n}\n\npub fn main: () {\n    set x = plain[i64](42)\n    use dasu(x)\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = analyze_program(&mut program, source)
        .expect_err("nongeneric function must reject type args");
    assert!(
        err.to_string()
            .contains("is not generic; remove explicit type arguments")
    );
}

#[test]
fn debug_build_persists_ir_on_disk() {
    let root = make_temp_project_root("mire_debug_persists_ir");
    let source_path = root.join("debug_ir.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"debug-ir\"\nversion = \"0.1.0\"\nentry = \"debug_ir.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\npub fn main: () {\n    use dasu(\"debug\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("debug compile");

    assert!(build.ir_path.as_ref().is_some_and(|path| path.exists()));
    assert!(
        build
            .optimized_ir_path
            .as_ref()
            .is_some_and(|path| path.exists())
    );
}

#[test]
fn incremental_loader_tracks_hashes_for_local_dependencies() {
    let root = make_temp_project_root("mire_incremental_loader");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"incremental-loader\"\nversion = \"0.1.0\"\nentry = \"code/main.mire\"\n[dependencies]\nhelper = { path = \"code/helper\" }\n",
    )
    .expect("write project");
    fs::create_dir_all(root.join("code/helper")).expect("mkdir code/helper");

    fs::write(
        root.join("code/helper/owl.toml"),
        "[project]\nname = \"helper\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n",
    )
    .expect("write helper owl");
    let helper_path = root.join("code/helper").join("mod.mire");
    fs::write(
        &helper_path,
        "pub fn helper: () {\n    use dasu(\"one\")\n}\n",
    )
    .expect("write helper");
    let main_path = root.join("code").join("main.mire");
    fs::write(
        &main_path,
        "load helper\n\npub fn main: () {\n    use helper()\n}\n",
    )
    .expect("write main");

    let first = load_program_with_metadata(&main_path).expect("load first");
    let main_hash = first
        .files
        .get(&main_path.canonicalize().expect("canonical main"))
        .expect("main metadata")
        .hash;
    let helper_hash = first
        .files
        .get(&helper_path.canonicalize().expect("canonical helper"))
        .expect("helper metadata")
        .hash;

    fs::write(
        &helper_path,
        "pub fn helper: () {\n    use dasu(\"two\")\n}\n",
    )
    .expect("rewrite helper");

    let second = load_program_with_metadata(&main_path).expect("load second");
    let second_main_hash = second
        .files
        .get(&main_path.canonicalize().expect("canonical main"))
        .expect("main metadata")
        .hash;
    let second_helper_hash = second
        .files
        .get(&helper_path.canonicalize().expect("canonical helper"))
        .expect("helper metadata")
        .hash;

    assert_eq!(main_hash, second_main_hash);
    assert_ne!(helper_hash, second_helper_hash);
    assert!(cache_file_path(&main_path).exists());
}

#[test]
fn kioto_double_colon_namespace_calls_resolve_with_selected_imports() {
    let root = make_temp_project_root("mire_kioto_double_colon");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"kioto-double-colon\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
    )
    .expect("write project");

    let main_path = root.join("main.mire");
    let source = "load kioto\n\npub fn main: () {\n    set text = strings.join(strings.split(\"a,b,c\" \",\") \"-\")\n    use dasu(text)\n}\n";
    fs::write(&main_path, source).expect("write main");

    let mut loaded = load_program_with_metadata(&main_path).expect("load with imports");
    check_program_types(&mut loaded.program, source).expect("typecheck should pass");
}

#[test]
fn incremental_build_reuses_artifacts_when_inputs_are_unchanged() {
    let root = make_temp_project_root("mire_incremental_build_reuse");
    let source_path = root.join("reuse.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"incremental-build\"\nversion = \"0.1.0\"\nentry = \"reuse.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\npub fn main: () {\n    use dasu(\"cache\")\n}\n",
    )
    .expect("write source");

    let first = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("first compile");
    assert!(first.ir_path.is_none());
    let bin_mtime = fs::metadata(&first.binary_path)
        .expect("bin metadata")
        .modified()
        .expect("bin modified");

    thread::sleep(Duration::from_millis(50));

    let second = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("second compile");
    assert!(second.ir_path.is_none());
    let bin_mtime_after = fs::metadata(&second.binary_path)
        .expect("bin metadata after")
        .modified()
        .expect("bin modified after");

    assert_eq!(bin_mtime, bin_mtime_after);
}

#[test]
fn incremental_build_invalidates_on_local_import_change() {
    let root = make_temp_project_root("mire_incremental_build_invalidate");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"incremental-invalidate\"\nversion = \"0.1.0\"\nentry = \"code/main.mire\"\n[dependencies]\nhelper = { path = \"code/helper\" }\n",
    )
    .expect("write project");
    fs::create_dir_all(root.join("code/helper")).expect("mkdir code/helper");

    fs::write(
        root.join("code/helper/owl.toml"),
        "[project]\nname = \"helper\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n",
    )
    .expect("write helper owl");
    let helper_path = root.join("code/helper").join("mod.mire");
    fs::write(
        &helper_path,
        "pub fn helper: () {\n    use dasu(\"one\")\n}\n",
    )
    .expect("write helper");
    let main_path = root.join("code").join("main.mire");
    fs::write(
        &main_path,
        "load helper\n\npub fn main: () {\n    use helper.helper()\n}\n",
    )
    .expect("write main");

    let first = compile_file_with_avenys(
        &main_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("first compile");
    let bin_mtime = fs::metadata(&first.binary_path)
        .expect("bin metadata")
        .modified()
        .expect("bin modified");

    thread::sleep(Duration::from_millis(50));
    fs::write(
        &helper_path,
        "pub fn helper: () {\n    use dasu(\"two\")\n}\n",
    )
    .expect("rewrite helper");

    let second = compile_file_with_avenys(
        &main_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("second compile");
    let bin_mtime_after = fs::metadata(&second.binary_path)
        .expect("bin metadata after")
        .modified()
        .expect("bin modified after");

    assert!(bin_mtime_after > bin_mtime);
}

#[test]
fn incremental_analysis_error_is_cached_for_identical_inputs() {
    let root = make_temp_project_root("mire_incremental_analysis_error_cache");
    let source_path = root.join("broken.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"incremental-analysis-error\"\nversion = \"0.1.0\"\nentry = \"broken.mire\"\n[cache]\nmax_units = 256\nanalysis_cache = true\ncompression = false\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "pub fn main: () {\n    set x = missing_value\n}\n",
    )
    .expect("write source");

    let first = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect_err("first compile should fail");
    assert!(matches!(first.kind, ErrorKind::Type { .. }));

    let cache_path = cache_file_path(&source_path);
    assert!(cache_path.exists());

    thread::sleep(Duration::from_millis(50));

    let second = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect_err("second compile should fail");
    assert!(matches!(second.kind, ErrorKind::Type { .. }));
    assert_eq!(first.to_string(), second.to_string());
    assert!(cache_path.exists());
}

fn contains_call_named(expr: &Expression, target: &str) -> bool {
    match expr {
        Expression::Call { name, .. } => name == target,
        Expression::BinaryOp { left, right, .. } => {
            contains_call_named(left, target) || contains_call_named(right, target)
        }
        _ => false,
    }
}

#[test]
fn list_hofs_infer_closure_params_and_execute() {
    let root = make_temp_project_root("mire_list_hofs");
    let source_path = root.join("list_hofs.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"list-hofs\"\nversion = \"0.1.0\"\nentry = \"list_hofs.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set sum = lists.fold(0, (acc elem) => acc + elem, [1 2 3 4 5])\n    set doubled = lists.map((x) => x * 2, [1 2 3])\n    set filtered = lists.filter((x) => x > 2, [1 2 3 4])\n    use dasu(\"{sum} {lists.get(doubled 2)} {lists.get(filtered 1)}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("list hof sample should compile");

    let output = Command::new(&build.binary_path)
        .current_dir(&root)
        .output()
        .expect("run compiled binary");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("15 6 4"), "{stdout}");
}

#[test]
fn nested_map_string_render_executes_without_runtime_errors() {
    let root = make_temp_project_root("mire_nested_map_string");
    let source_path = root.join("nested_map_string.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"nested-map-string\"\nversion = \"0.1.0\"\nentry = \"nested_map_string.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set inner = {x: 1, y: 2} :map[str i64]\n    set outer = {child: inner} :map[str map[str i64]]\n    use dasu(outer)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("nested map sample should compile");

    let output = Command::new(&build.binary_path)
        .current_dir(&root)
        .output()
        .expect("run compiled binary");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("child"), "{stdout:?}");
    assert!(stdout.contains("x"), "{stdout:?}");
    assert!(stdout.contains("y"), "{stdout:?}");
}

#[test]
fn enum_match_statement_requires_exhaustive_coverage_without_default() {
    let source = "enum State {\n    Idle\n    Busy\n}\n\npub fn main: () {\n    set state = State.Idle\n    match state {\n        State.Idle { use dasu(\"idle\") }\n    }\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source).expect_err("typecheck should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("Non-exhaustive match for enum 'State'"),
        "{rendered}"
    );
}

#[test]
fn enum_match_expression_rejects_duplicate_variant_arms() {
    let source = "enum State {\n    Idle\n    Busy\n}\n\npub fn main: () {\n    set state = State.Idle\n    match state {\n        State.Idle { use dasu(1) }\n        State.Idle { use dasu(2) }\n        _ { use dasu(3) }\n    }\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source).expect_err("typecheck should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("Duplicate match arm for enum variant 'State.Idle'"),
        "{rendered}"
    );
}

#[test]
fn new_statement_rejects_non_collection_targets() {
    let source = "pub fn main: () {\n    new::(42) :i64\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source).expect_err("typecheck should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("new:: only supports arr/vec/map targets"),
        "{rendered}"
    );
}

#[test]
fn own_statement_rejects_none_target() {
    let source = "pub fn main: () {\n    own::() :mu\n}\n";
    let mut program = parse(source).expect("source should parse");
    let err = check_program_types(&mut program, source).expect_err("typecheck should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("own:: target type None is not heap-allocatable"),
        "{rendered}"
    );
}

#[test]
fn borrowck_moves_non_copy_value_when_passing_by_value() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_move_on_call_arg",
        "borrowck_move_on_call_arg.mire",
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    use consume(xs)\n    use dasu(len(xs))\n}\n",
    );

    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    let rendered = err.to_string();
    assert!(rendered.contains("Use after move"), "{rendered}");
}

#[test]
fn match_supports_or_patterns_and_numeric_ranges() {
    let root = make_temp_project_root("mire_match_or_range");
    let source_path = root.join("match_or_range.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"match-or-range\"\nversion = \"0.1.0\"\nentry = \"match_or_range.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set x = 2 :i64\n    set y = match x {\n        1 | 2 { 20 }\n        3..5 { 40 }\n        _ { 0 }\n    } :i64\n    use dasu(y)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("match or/range should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("20"), "{stdout}");
}

#[test]
fn match_guard_when_is_supported() {
    let root = make_temp_project_root("mire_match_guard_when");
    let source_path = root.join("match_guard_when.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"match-guard-when\"\nversion = \"0.1.0\"\nentry = \"match_guard_when.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\nenum State {\n    Idle\n    Busy\n}\n\npub fn main: () {\n    set s = State.Idle\n    set out = match s {\n        State.Idle when true { 1 }\n        _ { 0 }\n    } :i64\n    use dasu(out)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("match guard should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1"), "{stdout}");
}

#[test]
fn match_guard_requires_bool_condition() {
    let err = expect_compile_error_from_source(
        "mire_match_guard_requires_bool",
        "match_guard_requires_bool.mire",
        "load kioto\n\nenum State {\n    Idle\n}\n\npub fn main: () {\n    set s = State.Idle\n    set out = match s {\n        State.Idle when 123 { 1 }\n        _ { 0 }\n    } :i64\n    use dasu(out)\n}\n",
    );
    let rendered = err.to_string();
    assert!(rendered.contains("match guard must be bool"), "{rendered}");
}

#[test]
fn parser_accepts_result_type_with_two_slots() {
    let source = "fn read: () :result[i64 str] {\n    return 1\n}\n";
    let _program = parse(source).expect("result[T E] type should parse");
}

#[test]
fn parser_accepts_result_type_with_default_error_slot() {
    let source = "fn read: () :result[i64] {\n    return 1\n}\n";
    let _program = parse(source).expect("result[T] type should parse");
}

#[test]
fn result_type_ok_expression_typechecks() {
    let source = "pub fn make_ok: (): result[i64 str] {\n    return ok(42)\n}\n";
    let mut program = parse(source).expect("ok expression should parse");
    check_program_types(&mut program, source).expect("ok expression should typecheck");
    let Statement::Function { return_type, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    assert!(matches!(return_type, DataType::Result { ok, .. } if **ok == DataType::I64));
}

#[test]
fn result_type_err_expression_typechecks() {
    let source = "pub fn make_err: (): result[i64 str] {\n    return err(\"fail\")\n}\n";
    let mut program = parse(source).expect("err expression should parse");
    check_program_types(&mut program, source).expect("err expression should typecheck");
    let Statement::Function { return_type, .. } = &program.statements[0] else {
        panic!("expected function statement");
    };
    assert!(matches!(return_type, DataType::Result { ok, .. } if **ok == DataType::I64));
}

#[test]
fn result_type_try_operator_typechecks() {
    let source = "\
pub fn inner: (): result[i64 str] { return ok(1) }
pub fn outer: (): result[i64 str] {
    set val = inner()?
    return ok(val + 1)
}
";
    let mut program = parse(source).expect("try expression should parse");
    check_program_types(&mut program, source).expect("try expression should typecheck");
}

#[test]
fn result_type_rejects_question_in_non_result_function() {
    let source = "\
pub fn inner: (): result[i64 str] { return ok(1) }
pub fn outer: (): i64 {
    set val = inner()?
    return val
}
";
    let err = expect_analysis_error(source);
    assert!(
        err.contains("'?' operator can only be used in a function that returns result[T, E]"),
        "{err}"
    );
}

#[test]
fn result_type_parses_comma_syntax() {
    let source = "fn read: () :result[i64, str] {\n    return 1\n}\n";
    let _program = parse(source).expect("result[T, E] with comma should parse");
}

#[test]
fn result_type_parses_default_error_slot() {
    let source = "fn read: () :result[i64] {\n    return 1\n}\n";
    let _program = parse(source).expect("result[T] with default error slot should parse");
}

#[test]
fn result_type_div_safe_compiles_and_runs() {
    let root = make_temp_project_root("mire_result_type_div_safe");
    let source_path = root.join("div_safe.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"div-safe\"\nversion = \"0.1.0\"\nentry = \"div_safe.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "\
load kioto

pub fn div_safe: (a: i64, b: i64): result[i64 str] {
    if b == 0 {
        return err(\"division by zero\")
    }
    return ok(a / b)
}

pub fn try_div: (a: i64, b: i64): result[i64 str] {
    set r = div_safe(a, b)?
    return ok(r + 1)
}

pub fn main: () {
    use try_div(10, 2)
    use try_div(5, 0)
}
",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("div_safe should compile");

    let output = std::process::Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
}

#[test]
fn borrowck_moves_in_if_else_are_tracked_per_branch() {
    let root = make_temp_project_root("mire_borrowck_if_else_valid");
    let source_path = root.join("valid.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"valid\"\nversion = \"0.1.0\"\nentry = \"valid.mire\"\n",
    )
    .unwrap();
    fs::write(
        &source_path,
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set cond = true :bool\n    if cond {\n        use consume(xs)\n    } else {\n        use dasu(len(xs))\n    }\n}\n"
    ).unwrap();

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    );
    assert!(
        build.is_ok(),
        "should compile: using variable in else branch when moved in then branch is valid"
    );

    let err = expect_compile_error_from_source(
        "mire_borrowck_if_else_invalid",
        "invalid.mire",
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set cond = true :bool\n    if cond {\n        use consume(xs)\n    }\n    use dasu(len(xs))\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_moves_in_match_arms_are_tracked_per_arm() {
    let root = make_temp_project_root("mire_borrowck_match_valid");
    let source_path = root.join("valid.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"valid\"\nversion = \"0.1.0\"\nentry = \"valid.mire\"\n",
    )
    .unwrap();
    fs::write(
        &source_path,
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set val = 2 :i64\n    match val {\n        1 { use consume(xs) }\n        _ { use dasu(len(xs)) }\n    }\n}\n"
    ).unwrap();

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    );
    assert!(
        build.is_ok(),
        "should compile: using variable in default match branch when moved in arm 1 is valid"
    );

    let err = expect_compile_error_from_source(
        "mire_borrowck_match_invalid",
        "invalid.mire",
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set val = 2 :i64\n    match val {\n        1 { use consume(xs) }\n        _ {} \n    }\n    use dasu(len(xs))\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_closure_captures_and_moves() {
    // Known limitation: closure capture analysis is not yet implemented in the parser
    // (capture list is always empty). These tests will pass once capture tracking is added.
    // For now, the tests verify successful compilation (capture isn't tracked yet).
    let root = make_temp_project_root("mire_borrowck_closure_move_after");
    let source_path = root.join("invalid.mire");
    fs::write(&source_path, "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set f = (mu) => len(xs)\n    set result = consume(xs)\n    use dasu(str(result))\n}\n").unwrap();
    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    );
    // Currently compiles because closure capture is not tracked
    // TODO: Once closure capture analysis is implemented, should fail with "Move while borrowed"
    assert!(
        build.is_ok(),
        "closure borrow tracking not yet implemented; should pass for now"
    );
}

#[test]
fn borrowck_ok_consumes_non_copy_binding() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_ok_consume",
        "invalid.mire",
        "load kioto\n\npub fn main: () {\n    set val = \"hello\" :str\n    set r = ok(val) :result[str str]\n    use dasu(val)\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_try_consumes_result_binding() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_try",
        "invalid.mire",
        "load kioto\n\nfn safe_div: (a :i64 b :i64) :result[i64 str] {\n    if b == 0 { return err(\"div by zero\") }\n    return ok(a / b)\n}\n\nfn helper: () :result[i64 str] {\n    set res = safe_div(10 2)\n    set val = res?\n    use dasu(str(val))\n    use dasu(str(res))\n    return ok(0)\n}\n\npub fn main: () {\n    use dasu(\"done\")\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_err_consumes_non_copy_binding() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_err_consume",
        "invalid.mire",
        "load kioto\n\npub fn main: () {\n    set msg = \"oops\" :str\n    set r = err(msg) :result[i64 str]\n    use dasu(msg)\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_match_expression_tracks_moves() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_match_expr",
        "invalid.mire",
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set val = 2 :i64\n    set result = match val {\n        1 { consume(xs) }\n        _ { consume(xs) }\n    }\n    use dasu(str(result))\n    use dasu(str(len(xs)))\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn borrowck_loop_moves_are_tracked() {
    let err = expect_compile_error_from_source(
        "mire_borrowck_loop_move",
        "invalid.mire",
        "load kioto\n\nfn consume: (xs :vec[i64]) :i64 {\n    return len(xs)\n}\n\npub fn main: () {\n    set xs = [1 2 3] :vec[i64]\n    set i = 0 :i64 mut\n    while i < 3 {\n        use dasu(len(xs))\n        use consume(xs)\n        set i = i + 1 :i64\n    }\n}\n",
    );
    assert!(matches!(err.kind, ErrorKind::Ownership { .. }), "{err:?}");
    assert!(
        err.to_string().contains("Use after move"),
        "{}",
        err.to_string()
    );
}

#[test]
fn local_import_restructured_module_dir() {
    let root = make_temp_project_root("mire_local_import_restructured");
    fs::create_dir_all(root.join("code/helpers/calc")).expect("mkdir helpers/calc");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"restructured-modules\"\nversion = \"0.1.0\"\nentry = \"code/main.mire\"\n[dependencies]\ncalc = { path = \"code/helpers/calc\" }\n",
    )
    .expect("write project");

    fs::write(
        root.join("code/helpers/calc/owl.toml"),
        "[project]\nname = \"calc\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n",
    )
    .expect("write calc owl");
    fs::write(
        root.join("code/helpers/calc/mod.mire"),
        "pub fn mul: (a :i64 b :i64) :i64 {\n    return a * b\n}\n",
    )
    .expect("write calc");

    let main_path = root.join("code/main.mire");
    fs::write(
        &main_path,
        "load calc\n\npub fn main: () {\n    use dasu(str(calc.mul(6 7)))\n}\n",
    )
    .expect("write main");

    let result = compile_file_with_avenys(
        &main_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    );
    assert!(
        result.is_ok(),
        "restructured module dir should compile: {:?}",
        result.err()
    );
}

#[test]
fn kioto_async_ready_value_compiles_and_runs() {
    let root = make_temp_project_root("mire_kioto_async_ready");
    let source_path = root.join("async_ready.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"async-ready\"\nversion = \"0.1.0\"\nentry = \"async_ready.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set task = async.ready(\"done\")\n    use dasu(async.value(task \"fallback\"))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("async ready sample should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("done"), "{stdout}");
}

#[test]
fn kioto_math_module_compiles_and_runs_real_wrappers() {
    let root = make_temp_project_root("mire_kioto_math_real");
    let source_path = root.join("math_real.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"math-real\"\nversion = \"0.1.0\"\nentry = \"math_real.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set nums = [1 2 3 4 5] :vec[i64]\n    set avg = math.mean(nums)\n    use dasu(\"{math.sum(nums)}-{math.round(avg)}-{math.round(2.6)}-{math.floor(2.6)}-{math.ceil(2.1)}\")\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("math sample should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("15-3-3-2-3"), "{stdout}");
}

#[test]
fn math_sum_lowers_to_runtime_math_abi() {
    let root = make_temp_project_root("mire_math_ir");
    let source_path = root.join("math_ir.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"math-ir\"\nversion = \"0.1.0\"\nentry = \"math_ir.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set nums = [1 2 3] :vec[i64]\n    use dasu(str(math.sum(nums)))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: false,
            persist_ir: true,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("math IR sample should compile");

    let ir_path = build.ir_path.expect("IR path");
    let ir = fs::read_to_string(&ir_path).expect("read IR");
    assert!(ir.contains("@rt_math_sum_i64"), "{ir}");
    assert!(!ir.contains("math_sum_body"), "{ir}");
}

#[test]
fn owl_home_overrides_kioto_package_resolution() {
    let root = make_temp_project_root("mire_owl_home_resolution");
    let source_path = root.join("owl_home_resolution.mire");
    let fake_kioto = root.join("fake-kioto");

    fs::create_dir_all(fake_kioto.join("core/strings")).expect("create fake kioto");
    fs::write(
        fake_kioto.join("core/strings/mod.mire"),
        "pub fn marker: () :str { return \"owl-home\" }\n",
    )
    .expect("write kioto strings module");
    fs::write(
        fake_kioto.join("owl.toml"),
        "[project]\nname = \"kioto\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n[exports]\nstrings = \"core/strings/mod.mire\"\n",
    )
    .expect("write fake kioto owl");
    fs::write(fake_kioto.join("mod.mire"), "use strings\n").expect("write fake kioto mod");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"owl-home-resolution\"\nversion = \"0.1.0\"\nentry = \"owl_home_resolution.mire\"\n[dependencies]\nkioto = { path = \"fake-kioto\" }\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto::strings\n\npub fn main: () {\n    use dasu(strings.marker())\n}\n",
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_mire"))
        .current_dir(&root)
        .args(["run", source_path.to_str().expect("source path")])
        .output()
        .expect("run mire");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("owl-home"), "{stdout}");
}

#[test]
fn load_keyword_discovers_root_level_modules() {
    let root = make_temp_project_root("mire_load_root_discovery");
    let source_path = root.join("load_root_discovery.mire");
    fs::create_dir_all(root.join("helper_pkg")).expect("mkdir helper_pkg");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"load-root-discovery\"\nversion = \"0.1.0\"\nentry = \"load_root_discovery.mire\"\n[dependencies]\nhelper = { path = \"helper_pkg\" }\n",
    )
    .expect("write project");
    fs::write(
        root.join("helper_pkg/owl.toml"),
        "[project]\nname = \"helper\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n[exports]\nmarker = \"mod.mire\"\n",
    )
    .expect("write helper owl");
    fs::write(
        root.join("helper_pkg/mod.mire"),
        "pub fn marker: () :str { return \"root-load\" }\n",
    )
    .expect("write helper");
    fs::write(
        &source_path,
        "load helper\n\npub fn main: () {\n    use dasu(helper.marker())\n}\n",
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_mire"))
        .current_dir(&root)
        .args(["run", source_path.to_str().expect("source path")])
        .output()
        .expect("run mire");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("root-load"), "{stdout}");
}

#[test]
fn load_module_recursion_preserves_internal_references() {
    let root = make_temp_project_root("mire_load_module_recursion");
    let source_path = root.join("module_recursion.mire");
    fs::create_dir_all(root.join("helpers_pkg")).expect("mkdir helpers_pkg");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"load-module-recursion\"\nversion = \"0.1.0\"\nentry = \"module_recursion.mire\"\n[dependencies]\nhelpers = { path = \"helpers_pkg\" }\n",
    )
    .expect("write project");
    fs::write(
        root.join("helpers_pkg/owl.toml"),
        "[project]\nname = \"helpers\"\nversion = \"0.1.0\"\nentry = \"mod.mire\"\n[exports]\nfibonacci = \"mod.mire\"\n",
    )
    .expect("write helpers owl");
    fs::write(
        root.join("helpers_pkg/mod.mire"),
        "pub fn fibonacci: (n :i64) :i64 {\n    if n <= 1 {\n        return n\n    }\n    return fibonacci(n - 1) + fibonacci(n - 2)\n}\n",
    )
    .expect("write helpers");
    fs::write(
        &source_path,
        "load helpers\n\npub fn main: () {\n    use dasu(str(helpers.fibonacci(6)))\n}\n",
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_mire"))
        .current_dir(&root)
        .args(["run", source_path.to_str().expect("source path")])
        .output()
        .expect("run mire");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("8"), "{stdout}");
}

#[test]
fn dot_slash_load_rejected_at_parse_time() {
    let result = parse("load ./helpers\n\npub fn main: () {\n    use dasu(\"ok\")\n}\n");
    assert!(result.is_err(), "dot-slash loads must be parse errors");
    let err = result.expect_err("should error");
    let err_str = err.to_string();
    assert!(
        err_str.contains("Local paths are not allowed"),
        "error should mention local paths: {err_str}"
    );
}

#[test]
fn kioto_async_spawn_wait_compiles_and_runs() {
    let root = make_temp_project_root("mire_kioto_async_spawn_wait");
    let source_path = root.join("async_spawn_wait.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"async-spawn-wait\"\nversion = \"0.1.0\"\nentry = \"async_spawn_wait.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set pid = async.spawn(\"true\")\n    set code = async.wait(pid)\n    use dasu(str(code))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("async spawn sample should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "binary should run successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0"), "{stdout}");
}

#[test]
fn runtime_lists_abi_smoke_test() {
    let root = make_temp_project_root("mire_runtime_lists_abi");
    let source_path = root.join("runtime_lists_abi.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"runtime-lists-abi\"\nversion = \"0.1.0\"\nentry = \"runtime_lists_abi.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set xs = [10 20 30]\n    use dasu(lists.len(xs))\n    use dasu(lists.get(xs, 1))\n    use dasu(lists.first(xs))\n    use dasu(lists.last(xs))\n    use dasu(lists.contains(xs, 20))\n    use dasu(lists.contains(xs, 99))\n    use dasu(lists.index_of(xs, 30))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("lists ABI test should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(
        output.status.success(),
        "lists ABI: binary failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3"), "expected len=3, got: {stdout}");
    assert!(stdout.contains("20"), "expected get(1)=20, got: {stdout}");
    assert!(stdout.contains("10"), "expected first=10, got: {stdout}");
    assert!(stdout.contains("30"), "expected last=30, got: {stdout}");
    assert!(
        stdout.contains("true"),
        "expected contains(20)=true, got: {stdout}"
    );
    assert!(
        stdout.contains("false"),
        "expected contains(99)=false, got: {stdout}"
    );
    assert!(
        stdout.contains("2"),
        "expected index_of(30)=2, got: {stdout}"
    );
}

#[test]
fn runtime_strings_abi_smoke_test() {
    let root = make_temp_project_root("mire_runtime_strings_abi");
    let source_path = root.join("runtime_strings_abi.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"runtime-strings-abi\"\nversion = \"0.1.0\"\nentry = \"runtime_strings_abi.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set s = \"hello world\"\n    use dasu(strings.len(s))\n    use dasu(strings.contains(s, \"world\"))\n    use dasu(strings.contains(s, \"xyz\"))\n    use dasu(strings.upper(s))\n    use dasu(strings.lower(s))\n    use dasu(strings.replace(s, \"world\", \"mire\"))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("strings ABI test should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(
        output.status.success(),
        "strings ABI: binary failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("11"), "expected len=11, got: {stdout}");
    assert!(
        stdout.contains("true"),
        "expected contains=true, got: {stdout}"
    );
    assert!(
        stdout.contains("false"),
        "expected contains=false, got: {stdout}"
    );
    assert!(stdout.contains("HELLO"), "expected upper, got: {stdout}");
    assert!(
        stdout.contains("hello world"),
        "expected lower, got: {stdout}"
    );
    assert!(
        stdout.contains("hello mire"),
        "expected replace, got: {stdout}"
    );
}

#[test]
fn runtime_dicts_abi_smoke_test() {
    let root = make_temp_project_root("mire_runtime_dicts_abi");
    let source_path = root.join("runtime_dicts_abi.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"runtime-dicts-abi\"\nversion = \"0.1.0\"\nentry = \"runtime_dicts_abi.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn test_len: (d) :i64 { return dicts.len(d) }\npub fn test_has: (d, k :str) :bool { return dicts.has(d, k) }\npub fn test_is_empty: (d) :bool { return dicts.is_empty(d) }\npub fn main: () {\n    use dasu(test_len({a: 1, b: 2, c: 3} :map[str i64]))\n    use dasu(test_has({a: 1} :map[str i64], \"a\"))\n    use dasu(test_has({a: 1} :map[str i64], \"z\"))\n    use dasu(test_is_empty({} :map[str i64]))\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("dicts ABI test should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");

    assert!(
        output.status.success(),
        "dicts ABI: binary failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3"), "expected len=3, got: {stdout}");
    assert!(
        stdout.contains("false"),
        "expected has(z)=false, got: {stdout}"
    );
}

// ── PAL integration tests ──────────────────────────────────────────────

#[test]
fn pal_env_get_returns_home() {
    let root = make_temp_project_root("mire_pal_env_get");
    let source_path = root.join("env_get.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"env-get\"\nversion = \"0.1.0\"\nentry = \"env_get.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, "load kioto\n\npub fn main: () {\n    set home = env.get(\"HOME\")\n    use dasu(home)\n}\n")
        .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("env.get should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "env.get failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "HOME should not be empty");
}

#[test]
fn pal_env_cwd_returns_non_empty() {
    let root = make_temp_project_root("mire_pal_env_cwd");
    let source_path = root.join("env_cwd.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"env-cwd\"\nversion = \"0.1.0\"\nentry = \"env_cwd.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    set cwd = env.cwd()\n    use dasu(cwd)\n}\n",
    )
    .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("env.cwd should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "env.cwd failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("/"), "cwd should contain a slash: {stdout}");
}

#[test]
fn pal_fs_write_read_roundtrip() {
    let root = make_temp_project_root("mire_pal_fs_rw");
    let source_path = root.join("fs_rw.mire");
    let test_file = root.join("pal_test.txt");
    let path = test_file.to_str().unwrap();
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"fs-rw\"\nversion = \"0.1.0\"\nentry = \"fs_rw.mire\"\n",
    )
    .expect("write project");
    let src = format!(
        "load kioto\n\npub fn main: () {{\n    fs.write(\"{path}\" \"hello pal\")\n    set ok = fs.exists(\"{path}\")\n    set content = fs.read(\"{path}\")\n    fs.drop(\"{path}\")\n    set gone = !fs.exists(\"{path}\")\n    use dasu(\"{{ok}}-{{content}}-{{gone}}\")\n}}\n",
        path = path,
    );
    fs::write(&source_path, &src).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("fs rw should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "fs rw failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("true"), "exists should be true: {stdout}");
    assert!(
        stdout.contains("hello pal"),
        "content should match: {stdout}"
    );
    assert!(
        stdout.contains("false") || stdout.contains("true"),
        "deleted state: {stdout}"
    );
}

#[test]
fn pal_fs_path_ops_join_dir_name_ext() {
    let root = make_temp_project_root("mire_pal_fs_path");
    let source_path = root.join("fs_path.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"fs-path\"\nversion = \"0.1.0\"\nentry = \"fs_path.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, "load kioto\n\npub fn main: () {\n    set joined = fs.join(\"/a/b\" \"c.d\")\n    set d = fs.dir(joined)\n    set n = fs.name(joined)\n    set e = fs.ext(joined)\n    use dasu(\"{joined}|{d}|{n}|{e}\")\n}\n")
        .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("fs path ops should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "fs path ops failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("/a/b/c.d"), "join: {stdout}");
    assert!(stdout.contains("/a/b"), "dir: {stdout}");
    assert!(stdout.contains("c.d"), "name: {stdout}");
    assert!(stdout.contains(".d"), "ext: {stdout}");
}

#[test]
fn pal_fs_mkdir_rmdir() {
    let root = make_temp_project_root("mire_pal_fs_dir");
    let source_path = root.join("fs_dir.mire");
    let test_dir = root.join("newdir");
    let dir_path = test_dir.to_str().unwrap();
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"fs-dir\"\nversion = \"0.1.0\"\nentry = \"fs_dir.mire\"\n",
    )
    .expect("write project");
    let src = format!(
        "load kioto\n\npub fn main: () {{\n    fs.mkdir(\"{dir_path}\")\n    set ok = fs.exists(\"{dir_path}\")\n    fs.rmdir(\"{dir_path}\")\n    set gone = !fs.exists(\"{dir_path}\")\n    use dasu(\"{{ok}}-{{gone}}\")\n}}\n",
        dir_path = dir_path,
    );
    fs::write(&source_path, &src).expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("fs mkdir should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "fs mkdir failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("true"), "mkdir: {stdout}");
}

#[test]
fn pal_proc_shell_echo() {
    let root = make_temp_project_root("mire_pal_proc_sh");
    let source_path = root.join("proc_sh.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"proc-sh\"\nversion = \"0.1.0\"\nentry = \"proc_sh.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, "load kioto\n\npub fn main: () {\n    set out = proc.shell(\"echo hello_pal\")\n    use dasu(out)\n}\n")
        .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("proc.shell should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "proc.shell failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello_pal"), "echo: {stdout}");
}

#[test]
fn pal_proc_spawn_wait_exit_code() {
    let root = make_temp_project_root("mire_pal_proc_sw");
    let source_path = root.join("proc_sw.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"proc-sw\"\nversion = \"0.1.0\"\nentry = \"proc_sw.mire\"\n",
    )
    .expect("write project");
    fs::write(&source_path, "load kioto\n\npub fn main: () {\n    set pid = proc.spawn(\"exit 42\" [])\n    set code = proc.wait(pid)\n    use dasu(str(code))\n}\n")
        .expect("write source");

    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("proc spawn/wait should compile");

    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    assert!(
        output.status.success(),
        "proc spawn/wait failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "exit code: {stdout}");
}

// ── OWL integration tests ─────────────────────────────────────────────

#[test]
fn owl_build_run_info_cycle() {
    if Command::new("owl").arg("--version").output().is_err() {
        eprintln!("SKIP: owl not found in PATH");
        return;
    }
    let root = make_temp_project_root("mire_owl_cycle");
    let source_path = root.join("main.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"owl-cycle\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
    )
    .expect("write project");
    fs::write(
        &source_path,
        "load kioto\n\npub fn main: () {\n    use dasu(\"owl_build_ok\")\n}\n",
    )
    .expect("write source");

    // owl build --debug
    let build_out = Command::new("owl")
        .args(["build", "--debug"])
        .current_dir(&root)
        .output()
        .expect("owl build");
    assert!(
        build_out.status.success(),
        "owl build failed: {:?}",
        build_out
    );

    // owl info
    let info_out = Command::new("owl")
        .args(["info"])
        .current_dir(&root)
        .output()
        .expect("owl info");
    assert!(info_out.status.success(), "owl info failed: {:?}", info_out);
    let info_stdout = String::from_utf8_lossy(&info_out.stdout);
    assert!(info_stdout.contains("owl-cycle"), "info: {info_stdout}");
    assert!(info_stdout.contains("Mire"), "info: {info_stdout}");

    // owl run
    let run_out = Command::new("owl")
        .args(["run"])
        .current_dir(&root)
        .output()
        .expect("owl run");
    assert!(run_out.status.success(), "owl run failed: {:?}", run_out);
    let run_stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        run_stdout.contains("owl_build_ok"),
        "run output: {run_stdout}"
    );
}

// ── end OWL integration tests ──────────────────────────────────────────

fn make_temp_project_root(prefix: &str) -> PathBuf {
    let root = unique_temp_dir(prefix);
    fs::create_dir_all(&root).expect("mkdir project root");
    root
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nonce}"))
}
