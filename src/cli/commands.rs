use crate::cli::*;
use mire::{
    BuildOptions, ImportMode, analyze_program, analyze_program_with_warnings_and_origins,
    compile_file_with_avenys, load_program_with_metadata,
};
use mire::compiler::WarningConfig;
use mire::error::diagnostic::Severity;
use mire::error::diagnostic::WarningFilter;
use mire::error::format::format_diagnostic;
use std::fs;
use std::path::Path;
use std::process::Command;

pub(crate) fn run_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (common, file, pass_through) = parse_run_options(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    set_owl_home_env(common.owl_home.as_ref());
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let options = BuildOptions {
        mode: common.mode,
        opt_level: common.opt_level,
        debug_dump: common.verbose,
        output: common
            .output
            .clone()
            .or_else(|| Some(default_binary_path(&path, common.mode))),
        emit_binary: true,
        persist_ir: false,
        import_mode: ImportMode::default(),
        cache: common.cache,
        warning_filter: common.warn.filter,
        deny_warnings: common.warn.deny,
        test_mode: false,
        module_paths: Vec::new(),
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
    println!("\nWarnings:");
    println!("  --show-warn           Show warning summary");
    println!("  --position            Show per-file warning locations");
    println!("  --no-warn <cat>       Suppress warning category (repeatable)");
    println!("  -W <code>             Promote warning to error");
    println!("  --deny <code>         Deny specific warning code");
    println!("\nOther:");
    println!("  --owl-home <path>     Override the Owl module cache root");
    println!("  --verbose, -v         Debug dump");
}

pub(crate) fn build_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|a| a == "--help") {
        build_help();
        return Ok(0);
    }
    let (common, file) = parse_common_with_file(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    set_owl_home_env(common.owl_home.as_ref());
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let options = BuildOptions {
        mode: common.mode,
        opt_level: common.opt_level,
        debug_dump: common.verbose,
        output: common
            .output
            .or_else(|| Some(default_binary_path(&path, common.mode))),
        emit_binary: true,
        persist_ir: false,
        import_mode: ImportMode::default(),
        cache: common.cache,
        warning_filter: common.warn.filter,
        deny_warnings: common.warn.deny,
        test_mode: false,
        module_paths: Vec::new(),
    };
    let build = compile_file_with_avenys(&path, &options)?;
    if !suppress_warn && !matches!(options.warning_filter, WarningFilter::Off) {
        emit_warnings(&build, common.warn.position, &common.warn.no_warn_cats);
    }
    println!("{}", build.binary_path.display());
    Ok(0)
}

pub(crate) fn check_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|a| a == "--help") {
        build_help();
        return Ok(0);
    }
    let (common, file) = parse_common_with_file(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    set_owl_home_env(common.owl_home.as_ref());
    let test_roots = read_test_roots(cwd);
    let suppress_warn = is_under_test_path(&path, &test_roots);
    let warn_filter_off = matches!(common.warn.filter, WarningFilter::Off);
    let source = fs::read_to_string(&path).map_err(runtime_err)?;
    let loaded = load_program_with_metadata(&path)?;
    let mut program = loaded.program;
    let mut analysis_program = program.clone();
    let _ = analyze_program(&mut analysis_program, &source)?;
    let report = analyze_program_with_warnings_and_origins(
        &mut program,
        &source,
        Some(&path.display().to_string()),
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
            has_error = filtered_diags.iter().any(|d| matches!(d.severity, Severity::Error));
        }
    } else {
        has_error = filtered_diags.iter().any(|d| matches!(d.severity, Severity::Error));
    }
    Ok(if has_error { 1 } else { 0 })
}

pub(crate) fn debug_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let options = parse_debug_options(cwd, args)?;
    let path = resolve_source_path(cwd, options.file.clone())?;
    set_owl_home_env(options.common.owl_home.as_ref());
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
            output: options
                .common
                .output
                .clone()
                .or_else(|| Some(default_binary_path(&path, options.common.mode))),
            emit_binary: !options.emit_ir_only,
            persist_ir: true,
            import_mode: ImportMode::default(),
            cache: options.common.cache,
            warning_filter: options.common.warn.filter,
            deny_warnings: options.common.warn.deny,
            test_mode: false,
            module_paths: Vec::new(),
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
