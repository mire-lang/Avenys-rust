use crate::cli::*;
use mire::error::diagnostic::WarningFilter;
use mire::{BuildOptions, ImportMode, compile_file_with_avenys};
use std::fs;
use std::path::Path;
use std::process::Command;

fn warn_legacy_link_flag(common: &CommonOptions) {
    if common.legacy_link_flag
        && matches!(
            common.error_format,
            mire::error::format::ErrorFormat::Human
        )
    {
        eprintln!(
            "warning[W0061 legacy_link_flag]: -L/--link without name=path is deprecated; use --link-dir <path>"
        );
    }
}

pub(crate) fn run_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: mire run [file] [options] [-- program-args]");
        println!("  --debug/--release     Select build profile");
        println!("  -O, --opt-level <n>   0|1|2|3|s|z");
        println!("  -o, --output <file>   Output executable");
        println!("  --output-dir <dir>    Output directory");
        println!("  --cache-dir <dir>     Incremental cache directory");
        println!("  --config <file>       Owl-generated normalized compiler config");
        println!("  -L, --link <name=path> Ecosystem library (repeatable)");
        println!("  -H, --hash <sha256:hex> Expected hash for the preceding --link");
        println!("  --link-dir <dir>      Native linker search directory");
        println!("  --bundle <mode>       static|static-strict|dynamic (default: static)");
        println!("  -l, --link-lib <name> Native library to link");
        println!("  --target <triple>     LLVM/Clang target triple");
        println!("  --error-format <fmt>  human|short|json|json-rendered-ansi|sarif");
        println!("  --color <policy>      auto|always|never");
        println!(
            "  --cpu <name>          LLVM CPU for instruction selection (native, x86-64-v3, ...);"
        );
        println!("  --artifact <bin|static|shared>  Output type");
        return Ok(0);
    }
    let (common, file, pass_through) = parse_run_options(cwd, args)?;
    warn_legacy_link_flag(&common);
    verify_ecosystem_hashes(&common)?;
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
        emit_warnings(
            &build,
            common.warn.position,
            &common.warn.no_warn_cats,
            common.error_format,
            common.color,
        );
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
    println!("  --runtime <tier>      Runtime tier: minimal|none");
    println!("  --target <triple>     LLVM/Clang target triple");
    println!(
        "  --cpu <name>          LLVM CPU for instruction selection (native, x86-64-v3, ...);"
    );
    println!("  -L, --link <name=path> Ecosystem library (repeatable)");
    println!("  -H, --hash <sha256:hex> Expected hash for the preceding --link");
    println!("  --link-dir <dir>      Native linker search directory");
    println!("  --bundle <mode>       static|static-strict|dynamic (default: static)");
    println!("  -l, --link-lib <name> Native library to link");
    println!("  --error-format <fmt>  human|short|json|json-rendered-ansi|sarif");
    println!("  --color <policy>      auto|always|never");
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
    warn_legacy_link_flag(&common);
    verify_ecosystem_hashes(&common)?;
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
        emit_warnings(
            &build,
            common.warn.position,
            &common.warn.no_warn_cats,
            common.error_format,
            common.color,
        );
    }
    println!("{}", build.binary_path.display());
    Ok(0)
}

pub(crate) fn debug_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: mire debug [file] [options]");
        println!("  --tokens, -t          Print lexer tokens");
        println!("  --ast, -p             Print parsed AST");
        println!("  --ir                  Emit IR without a binary");
        println!("  --run, -r             Run the debug binary");
        println!("  -O, --opt-level <n>   Optimization level");
        println!("  -L, --link <name=path> Ecosystem library (repeatable)");
        println!("  -H, --hash <sha256:hex> Expected hash for the preceding --link");
        println!("  --link-dir <dir>      Native linker search directory");
        println!("  --bundle <mode>       static|static-strict|dynamic (default: static)");
        println!("  -l, --link-lib <name> Native library to link");
        println!("  --target <triple>     LLVM/Clang target triple");
        println!("  --error-format <fmt>  human|short|json|json-rendered-ansi|sarif");
        println!("  --color <policy>      auto|always|never");
        println!(
            "  --cpu <name>          LLVM CPU for instruction selection (native, x86-64-v3, ...);"
        );
        return Ok(0);
    }
    let options = parse_debug_options(cwd, args)?;
    warn_legacy_link_flag(&options.common);
    verify_ecosystem_hashes(&options.common)?;
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
