use super::*;
use crate::parser::ast::DataType;
use std::hash::{Hash, Hasher};

pub(super) fn runtime_base() -> PathBuf {
    if let Ok(dir) = std::env::var("MIRE_RUNTIME_DIR") {
        let p = PathBuf::from(&dir);
        if p.join("runtime").exists() {
            return p;
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_dir.join("src/runtime").exists() {
        return manifest_dir.join("src");
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        if parent.join("runtime").exists() {
            return parent.to_path_buf();
        }
        if parent.join("../lib/mire/runtime").exists() {
            return parent.join("../lib/mire");
        }
    }
    manifest_dir.join("src")
}

pub(super) fn struct_field_llvm_type(dt: &DataType) -> &'static str {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64",
        DataType::I128 | DataType::U128 => "i128",
        DataType::I32 | DataType::U32 => "i32",
        DataType::I16 | DataType::U16 => "i16",
        DataType::I8 | DataType::U8 => "i8",
        DataType::F32 => "float",
        DataType::F64 => "double",
        DataType::Bool => "i1",
        DataType::None => "i64",
        DataType::Generic(_) => "i64",
        _ => "ptr",
    }
}

pub(super) fn struct_field_llvm_body_type(dt: &DataType) -> String {
    match dt {
        DataType::Array { element_type, size } => {
            format!("[{} x {}]", size, struct_field_llvm_body_type(element_type))
        }
        _ => struct_field_llvm_type(dt).to_string(),
    }
}

pub(super) fn struct_field_size(dt: &DataType) -> usize {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => 8,
        DataType::I128 | DataType::U128 => 16,
        DataType::I32 | DataType::U32 => 4,
        DataType::I16 | DataType::U16 => 2,
        DataType::I8 | DataType::U8 => 1,
        DataType::F32 => 4,
        DataType::F64 => 8,
        DataType::Bool => 1,
        DataType::None => 8,
        DataType::Array { element_type, size } => *size * struct_field_size(element_type),
        _ => 8,
    }
}

pub(super) fn generate_runtime_declarations(ir: &str) -> String {
    let mut out = String::new();
    let needed: &[(&str, &str)] = &[
        ("declare ptr @dasu(", "declare ptr @dasu(i64)"),
        ("declare i64 @rt_list_len(", "declare i64 @rt_list_len(ptr)"),
        (
            "declare i64 @rt_strings_len(",
            "declare i64 @rt_strings_len(ptr)",
        ),
        (
            "declare i64 @rt_dicts_len(",
            "declare i64 @rt_dicts_len(ptr)",
        ),
        (
            "declare ptr @rt_list_create(",
            "declare ptr @rt_list_create(i64, i64)",
        ),
        (
            "declare ptr @rt_list_push_i64(",
            "declare ptr @rt_list_push_i64(ptr, i64)",
        ),
        (
            "declare ptr @rt_list_push_ptr(",
            "declare ptr @rt_list_push_ptr(ptr, ptr)",
        ),
        (
            "declare ptr @rt_dicts_set_i64(",
            "declare ptr @rt_dicts_set_i64(ptr, ptr, i64)",
        ),
        (
            "declare ptr @rt_dicts_set(",
            "declare ptr @rt_dicts_set(ptr, ptr, ptr)",
        ),
        (
            "declare ptr @rt_dicts_set_with_kind(",
            "declare ptr @rt_dicts_set_with_kind(ptr, ptr, ptr, i64)",
        ),
        (
            "declare ptr @rt_dicts_keys(",
            "declare ptr @rt_dicts_keys(ptr)",
        ),
        (
            "declare ptr @rt_dicts_values(",
            "declare ptr @rt_dicts_values(ptr)",
        ),
        (
            "declare ptr @rt_dict_to_string(",
            "declare ptr @rt_dict_to_string(ptr)",
        ),
        (
            "declare i64 @rt_div_i64(",
            "declare i64 @rt_div_i64(i64, i64, i64, i64, ptr)",
        ),
        (
            "declare i64 @rt_rem_i64(",
            "declare i64 @rt_rem_i64(i64, i64, i64, i64, ptr)",
        ),
        (
            "declare void @rt_check_bounds_i64(",
            "declare void @rt_check_bounds_i64(i64, i64, i64, i64, ptr)",
        ),
        (
            "declare ptr @rt_closure_env_alloc(",
            "declare ptr @rt_closure_env_alloc(i64)",
        ),
        (
            "declare ptr @rt_math_range_i64(",
            "declare ptr @rt_math_range_i64(i64)",
        ),
        (
            "@.fmt_str =",
            "@.fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"",
        ),
        (
            "@.fmt_i64 =",
            "@.fmt_i64 = private unnamed_addr constant [5 x i8] c\"%ld\\0A\\00\"",
        ),
        (
            "@.fmt_f64 =",
            "@.fmt_f64 = private unnamed_addr constant [6 x i8] c\"%.6g\\0A\\00\"",
        ),
        (
            "@.fmt_float =",
            "@.fmt_float = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\"",
        ),
        (
            "@.fmt_bool_true =",
            "@.fmt_bool_true = private unnamed_addr constant [5 x i8] c\"true\\00\"",
        ),
        (
            "@.fmt_bool_false =",
            "@.fmt_bool_false = private unnamed_addr constant [6 x i8] c\"false\\00\"",
        ),
        (
            "@.fmt_i32 =",
            "@.fmt_i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\"",
        ),
        (
            "declare ptr @rt_i64_to_string(",
            "declare ptr @rt_i64_to_string(i64)",
        ),
        (
            "declare ptr @rt_f64_to_string(",
            "declare ptr @rt_f64_to_string(double)",
        ),
        (
            "declare ptr @rt_bool_to_string(",
            "declare ptr @rt_bool_to_string(i64)",
        ),
        (
            "declare ptr @rt_get_args(",
            "declare ptr @rt_get_args(i32, ptr)",
        ),
        ("declare i32 @printf(", "declare i32 @printf(ptr, ...)"),
        ("declare i32 @fflush(", "declare i32 @fflush(ptr)"),
        ("declare i32 @strcmp(", "declare i32 @strcmp(ptr, ptr)"),
        (
            "declare void @rt_managed_free(",
            "declare void @rt_managed_free(ptr)",
        ),
        (
            "declare ptr @rt_string_concat(",
            "declare ptr @rt_string_concat(ptr, ptr)",
        ),
        (
            "declare void @pal_proc_on(",
            "declare void @pal_proc_on(ptr)",
        ),
        ("@.argc =", "@.argc = global i32 0"),
        ("@.argv =", "@.argv = global ptr null"),
    ];
    for (search, decl) in needed {
        if !ir.contains(search) {
            out.push_str(decl);
            out.push('\n');
        }
    }
    out
}

pub(super) fn generate_struct_constructors(program: &crate::parser::ast::Program) -> String {
    let mut out = String::new();
    for stmt in &program.statements {
        if let Statement::Type { name, fields, .. } = stmt {
            let field_count = fields.len();
            if field_count == 0 {
                continue;
            }

            let param_types: Vec<&str> = fields
                .iter()
                .filter_map(|f| {
                    if let Statement::Let { data_type, .. } = f {
                        Some(struct_field_llvm_type(data_type))
                    } else {
                        None
                    }
                })
                .collect();
            let body_types: Vec<String> = fields
                .iter()
                .filter_map(|f| {
                    if let Statement::Let { data_type, .. } = f {
                        Some(struct_field_llvm_body_type(data_type))
                    } else {
                        None
                    }
                })
                .collect();

            let mut total_size = 0usize;
            for field in fields {
                if let Statement::Let { data_type, .. } = field {
                    total_size += struct_field_size(data_type);
                }
            }

            if param_types.is_empty() {
                continue;
            }

            let struct_ty = body_types.join(", ");
            let params: Vec<String> = param_types
                .iter()
                .enumerate()
                .map(|(i, ft)| format!("{} %{}", ft, i))
                .collect();

            let mut body = String::new();
            body.push_str(&format!("  %ptr = call ptr @malloc(i64 {total_size})\n"));
            for (i, field) in fields.iter().enumerate() {
                if let Statement::Let { data_type, .. } = field {
                    let bty = &body_types[i];
                    body.push_str(&format!(
                        "  %f{i}_ptr = getelementptr inbounds {{ {struct_ty} }}, ptr %ptr, i32 0, i32 {i}\n"
                    ));
                    match data_type {
                        DataType::Array { .. } => {
                            body.push_str(&format!("  %f{i}_loaded = load {bty}, ptr %{i}\n"));
                            body.push_str(&format!("  store {bty} %f{i}_loaded, ptr %f{i}_ptr\n"));
                        }
                        _ => {
                            body.push_str(&format!(
                                "  store {} %{i}, ptr %f{i}_ptr\n",
                                struct_field_llvm_type(data_type),
                            ));
                        }
                    }
                }
            }
            body.push_str("  ret ptr %ptr\n");

            out.push_str(&format!(
                "define ptr @{}({}) {{\nentry:\n{}}}\n\n",
                name,
                params.join(", "),
                body,
            ));
        }
    }
    if out.is_empty() {
        return String::new();
    }
    format!("declare ptr @malloc(i64)\n\n{}", out)
}

pub(super) fn dedup_llvm_declarations(ir: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for line in ir.lines() {
        // Force the correct signature for specific runtime functions whose
        // kioto extern declarations may not match what the codegen emits.
        let line = if line == "declare ptr @rt_get_args()" {
            "declare ptr @rt_get_args(i32, ptr)"
        } else {
            line
        };

        let should_skip = if let Some(rest) = line.strip_prefix("declare ") {
            if let Some(at_pos) = rest.find('@') {
                if let Some(paren_pos) = rest[at_pos..].find('(') {
                    let name = &rest[at_pos + 1..at_pos + paren_pos];
                    !seen.insert(name.to_string())
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if !should_skip {
            out.push(line);
        }
    }

    out.join("\n")
}

pub(super) fn c_object_hash(content: &str) -> u64 {
    let mut hasher = crate::incremental::FxHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn precompile_c_object(c_path: &str, cache_dir: &Path, runtime_base: &Path) -> Result<String> {
    let content = fs::read_to_string(c_path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::new(1, 1),
            message: format!("Could not read C source '{}': {}", c_path, err),
        })
    })?;
    let hash = c_object_hash(&content);
    fs::create_dir_all(cache_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not create cobjects dir: {}", err),
        })
    })?;
    let obj_path = cache_dir.join(format!("{:x}.o", hash));
    if !obj_path.exists() {
        let status = std::process::Command::new("clang")
            .args(["-c", "-O0", "-o"])
            .arg(&obj_path)
            .arg(c_path)
            .arg("-I")
            .arg(runtime_base.join("runtime"))
            .arg("-I")
            .arg(runtime_base.join("pal"))
            .status()
            .map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    span: crate::error::Span::unknown(),
                    message: format!("Failed to run clang for '{}': {}", c_path, err),
                })
            })?;
        if !status.success() {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("clang -c failed for '{}'", c_path),
            }));
        }
    }
    Ok(obj_path.to_string_lossy().to_string())
}

pub(super) fn progress_phase(phase: &str, _file: &str, elapsed_ms: u64, total_ms: u64) {
    if std::env::var("OWL_PROGRESS").is_ok() {
        eprintln!(
            "{{\"phase\":\"{}\",\"elapsed_ms\":{},\"total_ms\":{}}}",
            phase, elapsed_ms, total_ms
        );
    }
}

pub(super) fn apply_cfg_filter(program: &mut crate::parser::ast::Program) {
    let is_linux = cfg!(target_os = "linux");
    program.statements.retain(|stmt| {
        let attributes = match stmt {
            crate::parser::ast::Statement::Function { attributes, .. } => attributes,
            _ => return true,
        };
        let cfg_attr = attributes.iter().find(|a| a.name == "cfg");
        let Some(cfg_attr) = cfg_attr else {
            return true;
        };
        let target = cfg_attr.args.first().map(|a| a.value.as_str());
        match target {
            Some("linux") => is_linux,
            _ => false,
        }
    });
}

pub(super) fn inject_test_harness(program: &mut crate::parser::ast::Program) {
    use crate::parser::ast::{DataType, Expression, Identifier, Literal, Statement, Visibility};

    struct TestFn {
        name: String,
        section: String,
        ignored: bool,
    }

    let mut tests: Vec<TestFn> = Vec::new();
    for stmt in &program.statements {
        if let Statement::Function {
            name, attributes, ..
        } = stmt
            && attributes.iter().any(|a| a.name == "test")
        {
            let section = attributes
                .iter()
                .find(|a| a.name == "section")
                .and_then(|a| a.args.first())
                .map(|arg| arg.value.clone())
                .unwrap_or_default();
            let ignored = attributes.iter().any(|a| a.name == "ignore");
            tests.push(TestFn {
                name: name.clone(),
                section,
                ignored,
            });
        }
    }
    if tests.is_empty() {
        return;
    }

    let mut body: Vec<Statement> = Vec::new();
    let mut current_section = String::new();
    for test in &tests {
        if test.section != current_section {
            current_section = test.section.clone();
            if !current_section.is_empty() {
                body.push(Statement::Expression(Expression::Call {
                    name: "dasu".to_string(),
                    args: vec![Expression::Literal { lit: Literal::Str(format!(
                        "\n  [{}]",
                        current_section
                    )), line: 0, column: 0 }],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::None,
                }));
            }
        }
        if test.ignored {
            body.push(Statement::Expression(Expression::Call {
                name: "dasu".to_string(),
                args: vec![Expression::Literal { lit: Literal::Str(format!(
                    "  [SKIP] {}",
                    test.name
                )), line: 0, column: 0 }],
                type_args: Vec::new(),
                name_line: 0,
            name_column: 0,
            data_type: DataType::None,
            }));
        } else {
            body.push(Statement::Let {
                name: format!("_result_{}", test.name),
                data_type: DataType::Bool,
                value: Some(Expression::Call {
                    name: test.name.clone(),
                    args: Vec::new(),
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::Bool,
                }),
                is_constant: false,
                is_mutable: false,
                is_static: false,
                visibility: Visibility::Private,
                name_line: 0,
                name_column: 0,
            });
            let result_name = format!("_result_{}", test.name);
            body.push(Statement::If {
                condition: Expression::Identifier(Identifier {
                    name: result_name,
                    data_type: DataType::Bool,
                    line: 0,
                    column: 0,
                }),
                then_branch: vec![Statement::Expression(Expression::Call {
                    name: "dasu".to_string(),
                    args: vec![Expression::Literal { lit: Literal::Str(format!(
                        "  [PASS] {}",
                        test.name
                    )), line: 0, column: 0 }],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::None,
                })],
                else_branch: Some(vec![Statement::Expression(Expression::Call {
                    name: "dasu".to_string(),
                    args: vec![Expression::Literal { lit: Literal::Str(format!(
                        "  [FAIL] {}",
                        test.name
                    )), line: 0, column: 0 }],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::None,
                })]),
            });
        }
    }

    let harness = Statement::Function {
        name: "main".to_string(),
        attributes: Vec::new(),
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        params: Vec::new(),
        body,
        return_type: DataType::None,
        visibility: Visibility::Public,
        is_method: false,
        name_line: 0,
        name_column: 0,
    };
    program
        .statements
        .retain(|s| !matches!(s, Statement::Function { name, .. } if name == "main"));
    program.statements.push(harness);
}
