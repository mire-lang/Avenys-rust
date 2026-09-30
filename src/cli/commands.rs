use crate::cli::*;
use mire::compiler::WarningConfig;
use mire::error::diagnostic::Severity;
use mire::error::diagnostic::WarningFilter;
use mire::error::format::format_diagnostic;
use mire::{
    BuildOptions, ImportMode, analyze_program, analyze_program_with_warnings_and_origins,
    compile_file_with_avenys, load_program_with_metadata,
};
use std::fs;
use std::path::Path;
use std::process::Command;

pub(crate) fn run_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: mire run [file] [options] [-- program-args]");
        println!("  --debug/--release     Select build profile");
        println!("  -O, --opt-level <n>   0|1|2|3|s|z");
        println!("  -o, --output <file>   Output executable");
        println!("  --output-dir <dir>    Output directory");
        println!("  --cache-dir <dir>     Incremental cache directory");
        println!("  --config <file>       Owl-generated normalized compiler config");
        println!("  -L, --link <dir>      Native linker search directory");
        println!("  -l, --link-lib <name> Native library to link");
        println!("  --target <triple>     LLVM/Clang target triple");
        println!("  --artifact <bin|static|shared>  Output type");
        return Ok(0);
    }
    let (common, file, pass_through) = parse_run_options(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    configure_build_paths(cwd, &common, &path);
    set_lib_dir_env(&common.lib_dir);
    let c_defs = c_defs_for(cwd, common.config.as_deref())?;
    let mut c_defs = c_defs;
    apply_cli_c_defs(&mut c_defs, &common);
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let options = BuildOptions {
        mode: common.mode,
        opt_level: common.opt_level,
        debug_dump: common.verbose,
        output: common.output.clone().or_else(|| {
            common
                .output_dir
                .as_ref()
                .map(|dir| dir.join(path.file_stem().unwrap_or_default()))
                .or_else(|| Some(default_binary_path(&path, common.mode)))
        }),
        emit_binary: true,
        persist_ir: false,
        import_mode: ImportMode::default(),
        cache: common.cache,
        warning_filter: common.warn.filter,
        deny_warnings: common.warn.deny,
        test_mode: false,
        module_paths: Vec::new(),
        c_defs,
    };
    let build = compile_file_with_avenys(&path, &options)?;
    if !suppress_warn && !matches!(options.warning_filter, WarningFilter::Off) {
        emit_warnings(&build, common.warn.position, &common.warn.no_warn_cats);
    }
    let mut cmd = Command::new(&build.binary_path);
    for arg in pass_through {
        cmd.arg(arg);
    }
    let status = cmd.status().map_err(runtime_err)?;
    Ok(status.code().unwrap_or(1))
}

pub(crate) fn build_help() {
    println!("Usage: mire build [file] [options]");
    println!("\nProfiles:");
    println!("  --debug               Build profile debug (default)");
    println!("  --release             Build profile release");
    println!("  -O, --opt-level <n>   0|1|2|3|s|z");
    println!("\nOutput:");
    println!("  -o, --output <file>   Output binary path (default: <input>.out)");
    println!("  --output-dir <dir>    Output directory (default: bin/<profile>)");
    println!("  --cache-dir <dir>     Incremental cache directory (default: bin/.cache)");
    println!("  --config <file>       Owl-generated normalized compiler config");
    println!("  --artifact <type>     Output type: bin|static|shared (default: bin)");
    println!("  --runtime <tier>      Runtime tier: full|minimal|none");
    println!("  --target <triple>     LLVM/Clang target triple");
    println!("  -L, --link <dir>      Native linker search directory");
    println!("  -l, --link-lib <name> Native library to link");
    println!("\nWarnings:");
    println!("  --show-warn           Show warning summary");
    println!("  --position            Show per-file warning locations");
    println!("  --no-warn <cat>       Suppress warning category (repeatable)");
    println!("  -W <code>             Promote warning to error");
    println!("  --deny <code>         Deny specific warning code");
    println!("\nOther:");
    println!("  --lib-dir <path>      Extra fallback package directory");
    println!("  --verbose, -v         Debug dump");
}

pub(crate) fn build_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        build_help();
        return Ok(0);
    }
    let (common, file) = parse_common_with_file(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    configure_build_paths(cwd, &common, &path);
    set_lib_dir_env(&common.lib_dir);
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let mut c_defs = c_defs_for(cwd, common.config.as_deref())?;
    apply_cli_c_defs(&mut c_defs, &common);
    let options = BuildOptions {
        mode: common.mode,
        opt_level: common.opt_level,
        debug_dump: common.verbose,
        output: common.output.or_else(|| {
            common
                .output_dir
                .as_ref()
                .map(|dir| dir.join(path.file_stem().unwrap_or_default()))
                .or_else(|| Some(default_binary_path(&path, common.mode)))
        }),
        emit_binary: true,
        persist_ir: false,
        import_mode: ImportMode::default(),
        cache: common.cache,
        warning_filter: common.warn.filter,
        deny_warnings: common.warn.deny,
        test_mode: false,
        module_paths: Vec::new(),
        c_defs,
    };
    let build = compile_file_with_avenys(&path, &options)?;
    if !suppress_warn && !matches!(options.warning_filter, WarningFilter::Off) {
        emit_warnings(&build, common.warn.position, &common.warn.no_warn_cats);
    }
    println!("{}", build.binary_path.display());
    Ok(0)
}

#[allow(dead_code)]
pub(crate) fn check_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|a| a == "--help") {
        build_help();
        return Ok(0);
    }
    let (common, file) = parse_common_with_file(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    set_lib_dir_env(&common.lib_dir);
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let warn_filter_off = matches!(common.warn.filter, WarningFilter::Off);
    let source = fs::read_to_string(&path).map_err(runtime_err)?;
    let source_filename = path.display().to_string();
    let check = || -> Result<i32, MireError> {
        let loaded = load_program_with_metadata(&path)?;
        let mut program = loaded.program;
        let mut analysis_program = program.clone();
        let _ = analyze_program(&mut analysis_program, &source)?;
        let report = analyze_program_with_warnings_and_origins(
            &mut program,
            &source,
            Some(&source_filename),
            WarningConfig {
                filter: common.warn.filter,
                deny: common.warn.deny,
            },
            &loaded.statement_origins,
            &path,
        )?;

        let filtered_diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| !should_suppress(d.code.name(), &common.warn.no_warn_cats))
            .cloned()
            .collect();
        let mut has_error = false;
        if !suppress_warn && !warn_filter_off {
            if common.warn.position {
                for diagnostic in &filtered_diags {
                    eprintln!("{}", format_diagnostic(diagnostic, true));
                    if matches!(diagnostic.severity, Severity::Error) {
                        has_error = true;
                    }
                }
            } else {
                print_warning_summary(&filtered_diags);
                has_error = filtered_diags
                    .iter()
                    .any(|d| matches!(d.severity, Severity::Error));
            }
        } else {
            has_error = filtered_diags
                .iter()
                .any(|d| matches!(d.severity, Severity::Error));
        }
        Ok(if has_error { 1 } else { 0 })
    };
    check().map_err(|err| err.ensure_context(&source_filename, &source))
}

pub(crate) fn debug_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: mire debug [file] [options]");
        println!("  --tokens, -t          Print lexer tokens");
        println!("  --ast, -p             Print parsed AST");
        println!("  --ir                  Emit IR without a binary");
        println!("  --run, -r             Run the debug binary");
        println!("  -O, --opt-level <n>   Optimization level");
        println!("  -L, --link <dir>      Native linker search directory");
        println!("  -l, --link-lib <name> Native library to link");
        println!("  --target <triple>     LLVM/Clang target triple");
        return Ok(0);
    }
    let options = parse_debug_options(cwd, args)?;
    let path = resolve_source_path(cwd, options.file.clone())?;
    configure_build_paths(cwd, &options.common, &path);
    set_lib_dir_env(&options.common.lib_dir);
    let c_defs = c_defs_for(cwd, options.common.config.as_deref())?;
    let mut c_defs = c_defs;
    apply_cli_c_defs(&mut c_defs, &options.common);
    let source = fs::read_to_string(&path).map_err(runtime_err)?;

    if options.show_tokens {
        let tokens = mire::lexer::tokenize(&source).map_err(|err| {
            err.with_source(source.clone())
                .with_filename(path.display().to_string())
        })?;
        for token in &tokens {
            println!("{:?}", token);
        }
    }

    if options.show_ast {
        let program = mire::parser::parse(&source).map_err(|err| {
            err.with_source(source.clone())
                .with_filename(path.display().to_string())
        })?;
        println!("{:#?}", program);
    }

    let build = compile_file_with_avenys(
        &path,
        &BuildOptions {
            mode: options.common.mode,
            opt_level: options.common.opt_level,
            debug_dump: true,
            output: options.common.output.clone().or_else(|| {
                options
                    .common
                    .output_dir
                    .as_ref()
                    .map(|dir| dir.join(path.file_stem().unwrap_or_default()))
                    .or_else(|| Some(default_binary_path(&path, options.common.mode)))
            }),
            emit_binary: !options.emit_ir_only,
            persist_ir: true,
            import_mode: ImportMode::default(),
            cache: options.common.cache,
            warning_filter: options.common.warn.filter,
            deny_warnings: options.common.warn.deny,
            test_mode: false,
            module_paths: Vec::new(),
            c_defs,
        },
    )?;

    if let Some(ir) = &build.ir_path {
        println!("IR: {}", ir.display());
    }
    if let Some(ir) = &build.optimized_ir_path {
        println!("OPT IR: {}", ir.display());
    }
    if options.run_binary && !options.emit_ir_only {
        let status = Command::new(&build.binary_path)
            .status()
            .map_err(runtime_err)?;
        return Ok(status.code().unwrap_or(1));
    }
    Ok(0)
}

/// LSP foundation (Capa 1 — lexical). Emits a JSON document with the raw
/// token stream so editors (e.g. token) can paint lexical highlighting
/// immediately without running the full pipeline. This is the read-only,
/// additive API surface that owl/LSP connectors will build on.
///
/// Usage: `mire --lsp [file] [--root <path>]`
///   - no file: project mode (resolve owl.toml root) — reserved for later.
///   - file given: focus mode — tokenize that single file only.
#[allow(dead_code)]
pub(crate) fn lsp_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let mut file: Option<String> = None;
    let mut _root: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--root" && i + 1 < args.len() {
            _root = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if !a.starts_with('-') && file.is_none() {
            file = Some(a.clone());
        }
        i += 1;
    }

    let path = match &file {
        Some(f) => resolve_source_path(cwd, Some(f.clone()))?,
        None => {
            eprintln!("lsp: project mode not yet implemented; pass a file");
            return Ok(1);
        }
    };
    let source = fs::read_to_string(&path).map_err(runtime_err)?;
    set_lib_dir_env(&None);

    let tokens = mire::lexer::tokenize(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;

    // Manual JSON (no extra deps): Capa 1 lexical tokens + Capa 2 diagnostics.
    let mut s = String::from("{\"tokens\":[");
    for (idx, tok) in tokens.iter().enumerate() {
        if idx > 0 {
            s.push(',');
        }
        let kind = match tok.ttype {
            mire::lexer::TokenType::Ident => "ident",
            mire::lexer::TokenType::IntLit
            | mire::lexer::TokenType::FloatLit
            | mire::lexer::TokenType::BoolLit => "number",
            mire::lexer::TokenType::StrLit
            | mire::lexer::TokenType::CharLit
            | mire::lexer::TokenType::NoneLit => "string",
            mire::lexer::TokenType::Load
            | mire::lexer::TokenType::Use
            | mire::lexer::TokenType::Module => "loaduse",
            mire::lexer::TokenType::If
            | mire::lexer::TokenType::Elif
            | mire::lexer::TokenType::Else
            | mire::lexer::TokenType::While
            | mire::lexer::TokenType::For
            | mire::lexer::TokenType::Fn
            | mire::lexer::TokenType::Struct
            | mire::lexer::TokenType::Skill
            | mire::lexer::TokenType::Enum
            | mire::lexer::TokenType::Impl
            | mire::lexer::TokenType::Trait
            | mire::lexer::TokenType::Match
            | mire::lexer::TokenType::Return
            | mire::lexer::TokenType::Pub
            | mire::lexer::TokenType::Priv
            | mire::lexer::TokenType::Const
            | mire::lexer::TokenType::Cons
            | mire::lexer::TokenType::Mut
            | mire::lexer::TokenType::Extern
            | mire::lexer::TokenType::Lib
            | mire::lexer::TokenType::Set
            | mire::lexer::TokenType::Type
            | mire::lexer::TokenType::In
            | mire::lexer::TokenType::Do
            | mire::lexer::TokenType::As
            | mire::lexer::TokenType::Is
            | mire::lexer::TokenType::Of
            | mire::lexer::TokenType::To
            | mire::lexer::TokenType::At
            | mire::lexer::TokenType::SelfToken
            | mire::lexer::TokenType::Extends
            | mire::lexer::TokenType::Super
            | mire::lexer::TokenType::Break
            | mire::lexer::TokenType::Continue
            | mire::lexer::TokenType::Unsafe
            | mire::lexer::TokenType::Asm
            | mire::lexer::TokenType::NewKw
            | mire::lexer::TokenType::DropKw
            | mire::lexer::TokenType::MoveKw
            | mire::lexer::TokenType::OwnKw => "keyword",
            _ => "operator",
        };
        let v = match &tok.value {
            Some(x) => format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\"")),
            None => "null".to_string(),
        };
        s.push_str(&format!(
            "{{\"k\":\"{}\",\"v\":{},\"l\":{},\"c\":{}}}",
            kind, v, tok.line, tok.column
        ));
    }
    s.push_str("],\"diagnostics\":[");

    // Capa 2 — run the analysis pipeline and surface diagnostics. Errors during
    // analysis (e.g. unresolved imports) are themselves reported as a diagnostic
    // so the editor always gets a structured result.
    let diag_result: Result<Vec<String>, MireError> = (|| {
        let loaded = load_program_with_metadata(&path)?;
        let mut program = loaded.program;
        let mut analysis_program = program.clone();
        let _ = analyze_program(&mut analysis_program, &source);
        let report = analyze_program_with_warnings_and_origins(
            &mut program,
            &source,
            Some(&path.display().to_string()),
            WarningConfig {
                filter: WarningFilter::All,
                deny: std::collections::HashSet::new(),
            },
            &loaded.statement_origins,
            &path,
        )?;
        let mut out = Vec::new();
        for d in &report.diagnostics {
            if d.span.is_unknown() {
                continue;
            }
            let sev = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                _ => "info",
            };
            let code = d.code.name();
            let msg = d.message.replace('\\', "\\\\").replace('"', "\\\"");
            out.push(format!(
                "{{\"sev\":\"{}\",\"code\":\"{}\",\"msg\":\"{}\",\"l\":{},\"c\":{},\"len\":1}}",
                sev, code, msg, d.span.line, d.span.column
            ));
        }
        Ok(out)
    })();
    match diag_result {
        Ok(items) => {
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    s.push(',');
                }
                s.push_str(item);
            }
        }
        Err(err) => {
            let msg = format!("{}", err)
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            s.push_str(&format!(
                "{{\"sev\":\"error\",\"code\":\"pipeline\",\"msg\":\"{}\",\"l\":1,\"c\":1,\"len\":1}}",
                msg
            ));
        }
    }
    s.push_str("]}");
    println!("{}", s);
    Ok(0)
}
