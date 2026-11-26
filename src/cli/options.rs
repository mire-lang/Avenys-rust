use crate::cli::*;
use mire::error::diagnostic::{DiagnosticCode, WarningFilter};
use mire::{
    BuildMode, CacheOverrides, LibType, MireError, OptLevel, default_output_dir, load_config_file,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct CommonOptions {
    pub(crate) mode: BuildMode,
    pub(crate) opt_level: OptLevel,
    pub(crate) output: Option<PathBuf>,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) cache_dir: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) cache: CacheOverrides,
    pub(crate) lib_dir: Option<String>,
    pub(crate) artifact: Option<LibType>,
    pub(crate) warn: WarningCliOptions,
    pub(crate) error_format: mire::error::format::ErrorFormat,
    pub(crate) color: mire::error::format::ColorPolicy,
    pub(crate) verbose: bool,
    pub(crate) target: Option<String>,
    /// LLVM CPU for instruction selection (e.g. `native`, `x86-64-v3`).
    pub(crate) cpu: Option<String>,
    pub(crate) link_dirs: Vec<String>,
    pub(crate) link_libs: Vec<String>,
    pub(crate) ecosystem_libs: Vec<(String, String)>,
    /// One entry per `ecosystem_libs` element; `-H` sets the hash expected for
    /// the most recent `--link`.
    pub(crate) ecosystem_hashes: Vec<Option<String>>,
    pub(crate) legacy_link_flag: bool,
    pub(crate) runtime: Option<mire::RuntimeTier>,
    /// Native link preference from `--bundle`; `None` keeps the manifest value
    /// (default `static`).
    pub(crate) bundle: Option<mire::BundleMode>,
    pub(crate) nostartfiles: bool,
    pub(crate) nostdlib: bool,
    /// Native C sources, from `--c-source`. Repeated.
    pub(crate) c_sources: Vec<String>,
    /// Native C include dirs, from `--c-include`. Repeated.
    pub(crate) c_includes: Vec<String>,
    /// Raw C compiler flags, from `--cflag`. Repeated.
    pub(crate) cflags: Vec<String>,
    /// Native libraries to link, from `--c-lib`. Repeated.
    pub(crate) c_libs: Vec<String>,
    /// Preprocessor defines, from `--define name` or `--define name=value`.
    pub(crate) defines: Vec<String>,
    /// Security mode from `--security-mode open|strict`.
    pub(crate) security_mode: Option<mire::SecurityMode>,
    /// Extern allowlist patterns from `--extern`. Repeated, `*` suffix allowed.
    pub(crate) externs: Vec<String>,
    /// Allowed extern libraries from `--extern-lib`. Repeated.
    pub(crate) extern_libs: Vec<String>,
    /// Allowed macros from `--allow-macro`. Repeated.
    pub(crate) allow_macros: Vec<String>,
    /// `--allow-unsafe` permits unsafe blocks.
    pub(crate) allow_unsafe: bool,
    /// Macro file bindings, `--macro name=path`. Repeated.
    pub(crate) macro_bindings: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct WarningCliOptions {
    pub(crate) filter: WarningFilter,
    pub(crate) deny: HashSet<DiagnosticCode>,
    pub(crate) position: bool,
    pub(crate) no_warn_cats: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DebugOptions {
    pub(crate) common: CommonOptions,
    pub(crate) file: Option<String>,
    pub(crate) show_tokens: bool,
    pub(crate) show_ast: bool,
    pub(crate) run_binary: bool,
    pub(crate) emit_ir_only: bool,
}

pub(crate) fn parse_run_options(
    cwd: &Path,
    args: &[String],
) -> Result<(CommonOptions, Option<String>, Vec<String>), MireError> {
    let mut split = 0usize;
    while split < args.len() {
        if args[split] == "--" {
            break;
        }
        split += 1;
    }
    let (left, right) = if split < args.len() {
        (&args[..split], args[split + 1..].to_vec())
    } else {
        (args, Vec::new())
    };

    let (common, file) = parse_common_with_file(cwd, left)?;
    Ok((common, file, right))
}

pub(crate) fn parse_common_with_file(
    _cwd: &Path,
    args: &[String],
) -> Result<(CommonOptions, Option<String>), MireError> {
    let mut mode = BuildMode::Debug;
    let mut opt_level = OptLevel::O0;
    let mut output = None;
    let mut output_dir = None;
    let mut file = None;
    let mut cache = CacheOverrides::default();
    let mut lib_dir = None;
    let mut cache_dir = None;
    let mut config = None;
    let mut artifact = None;
    let mut verbose = false;
    let mut show_warn = false;
    let mut position = false;
    let mut warn_codes = HashSet::new();
    let mut deny_codes = HashSet::new();
    let mut no_warn_cats: Vec<String> = Vec::new();
    let mut target = None;
    let mut cpu = None;
    let mut error_format = mire::error::format::ErrorFormat::default();
    let mut color = mire::error::format::ColorPolicy::default();
    let mut link_dirs = Vec::new();
    let mut link_libs = Vec::new();
    let mut ecosystem_libs = Vec::new();
    let mut ecosystem_hashes: Vec<Option<String>> = Vec::new();
    let mut legacy_link_flag = false;
    let mut runtime = None;
    let mut bundle = None;
    let mut nostartfiles = false;
    let mut nostdlib = false;
    let mut c_sources = Vec::new();
    let mut c_includes = Vec::new();
    let mut cflags = Vec::new();
    let mut c_libs = Vec::new();
    let mut defines = Vec::new();
    let mut security_mode = None;
    let mut externs = Vec::new();
    let mut extern_libs = Vec::new();
    let mut allow_macros = Vec::new();
    let mut allow_unsafe = false;
    let mut macro_bindings: Vec<(String, String)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--debug" => {
                mode = BuildMode::Debug;
                if matches!(opt_level, OptLevel::O0) {
                    opt_level = OptLevel::O0;
                }
            }
            "--release" => {
                mode = BuildMode::Release;
                if matches!(opt_level, OptLevel::O0) {
                    opt_level = OptLevel::O3;
                }
            }
            "-O" | "--opt-level" => {
                i += 1;
                let level = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing optimization level after -O/--opt-level"))?;
                opt_level = OptLevel::parse(level)
                    .ok_or_else(|| cli_msg("Invalid optimization level, use 0/1/2/3/s/z"))?;
            }
            flag if flag.starts_with("-O") && flag.len() > 2 => {
                opt_level = OptLevel::parse(&flag[2..])
                    .ok_or_else(|| cli_msg("Invalid optimization level, use 0/1/2/3/s/z"))?;
            }
            "-o" | "--output" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing output path after -o/--output"))?;
                output = Some(PathBuf::from(value));
            }
            "--output-dir" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing output directory after --output-dir"))?;
                output_dir = Some(PathBuf::from(value));
            }
            "--cache-dir" | "--cache" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing cache directory after --cache-dir"))?;
                cache_dir = Some(PathBuf::from(value));
            }
            "--lib-dir" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing value for --lib-dir"))?;
                lib_dir = Some(value.to_string());
            }
            "--config" => {
                i += 1;
                config =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        cli_msg("Missing config path after --config")
                    })?));
            }
            "--artifact" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    cli_msg("Missing artifact after --artifact (bin|static|shared)")
                })?;
                artifact = Some(
                    LibType::parse(value)
                        .ok_or_else(|| cli_msg("Invalid artifact, use bin|static|shared"))?,
                );
            }
            "--target" => {
                i += 1;
                target = Some(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing target after --target"))?
                        .clone(),
                );
            }
            "--cpu" => {
                i += 1;
                cpu = Some(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing cpu after --cpu"))?
                        .clone(),
                );
            }
            "--error-format" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing format after --error-format"))?;
                error_format = mire::error::format::ErrorFormat::parse(value).ok_or_else(|| {
                    cli_msg("Invalid --error-format, use human|short|json|json-rendered-ansi|sarif")
                })?;
            }
            "--message-format" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing format after --message-format"))?;
                error_format = match value.as_str() {
                    "human" => mire::error::format::ErrorFormat::Human,
                    "json" => mire::error::format::ErrorFormat::Json,
                    other => {
                        return Err(cli_msg(&format!(
                            "Invalid --message-format '{other}', use human|json"
                        )));
                    }
                };
            }
            "--color" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing policy after --color"))?;
                color = mire::error::format::ColorPolicy::parse(value)
                    .ok_or_else(|| cli_msg("Invalid --color, use auto|always|never"))?;
            }
            flag if flag.starts_with("--error-format=") => {
                let value = &flag["--error-format=".len()..];
                error_format = mire::error::format::ErrorFormat::parse(value).ok_or_else(|| {
                    cli_msg("Invalid --error-format, use human|short|json|json-rendered-ansi|sarif")
                })?;
            }
            flag if flag.starts_with("--message-format=") => {
                let value = &flag["--message-format=".len()..];
                error_format = match value {
                    "human" => mire::error::format::ErrorFormat::Human,
                    "json" => mire::error::format::ErrorFormat::Json,
                    other => {
                        return Err(cli_msg(&format!(
                            "Invalid --message-format '{other}', use human|json"
                        )));
                    }
                };
            }
            flag if flag.starts_with("--color=") => {
                let value = &flag["--color=".len()..];
                color = mire::error::format::ColorPolicy::parse(value)
                    .ok_or_else(|| cli_msg("Invalid --color, use auto|always|never"))?;
            }
            "-L" | "--link" | "--link-dir" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing path after -L/--link/--link-dir"))?
                    .clone();
                if args[i - 1] == "--link-dir" {
                    link_dirs.push(value);
                } else if let Some((name, path)) = value.split_once('=') {
                    if name.is_empty() || path.is_empty() {
                        return Err(cli_msg(&format!(
                            "-L/--link expects name=path, got '{value}'"
                        )));
                    }
                    ecosystem_libs.push((name.to_string(), path.to_string()));
                    ecosystem_hashes.push(None);
                } else if Path::new(&value).is_dir() {
                    legacy_link_flag = true;
                    link_dirs.push(value);
                } else {
                    return Err(cli_msg(&format!(
                        "Invalid -L/--link '{value}': use name=path for an ecosystem library, --link-dir <path> for a native linker directory, or pass an existing directory"
                    )));
                }
            }
            "-l" | "--link-lib" => {
                i += 1;
                link_libs.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing library after -l/--link-lib"))?
                        .clone(),
                );
            }
            "--runtime" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing runtime tier after --runtime"))?;
                runtime = Some(
                    mire::RuntimeTier::parse(value)
                        .ok_or_else(|| cli_msg("Runtime must be minimal or none"))?,
                );
            }
            "--bundle" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing bundle mode after --bundle"))?;
                bundle = Some(mire::BundleMode::parse(value).ok_or_else(|| {
                    cli_msg("Bundle mode must be static, static-strict, or dynamic")
                })?);
            }
            "-H" | "--hash" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing hash after -H/--hash"))?;
                if !value.starts_with("sha256:") {
                    return Err(cli_msg("--hash expects sha256:<hex>"));
                }
                let slot = ecosystem_hashes
                    .last_mut()
                    .ok_or_else(|| cli_msg("--hash must follow a --link name=path"))?;
                if slot.is_some() {
                    return Err(cli_msg(
                        "each --link accepts at most one --hash; repeat --link for another library",
                    ));
                }
                *slot = Some(value.clone());
            }
            "--nostartfiles" => nostartfiles = true,
            "--nostdlib" => nostdlib = true,
            "--cache-max-units" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing value for --cache-max-units"))?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| cli_msg("Invalid --cache-max-units value"))?;
                cache.max_units = Some(parsed);
            }
            "--no-analysis-cache" => cache.analysis_cache = Some(false),
            "--analysis-cache" => cache.analysis_cache = Some(true),
            "--show-warn" | "--sh-warn" => show_warn = true,
            "--position" | "--pos" => position = true,
            "--warnings-as-errors" | "--deny-warnings" => {
                for code in [
                    DiagnosticCode::W0001,
                    DiagnosticCode::W0002,
                    DiagnosticCode::W0004,
                    DiagnosticCode::W0005,
                    DiagnosticCode::W0034,
                    DiagnosticCode::W0039,
                ] {
                    deny_codes.insert(code);
                }
            }
            "--no-warn" => {
                i += 1;
                let cat = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing warning category after --no-warn"))?;
                no_warn_cats.push(cat.clone());
            }
            "-W" => {
                i += 1;
                let code = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing warning code after -W"))?;
                warn_codes.insert(parse_warning_code(code)?);
            }
            "--deny" => {
                i += 1;
                let code = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing warning code after --deny"))?;
                deny_codes.insert(parse_warning_code(code)?);
            }
            "--verbose" | "-v" => verbose = true,
            "--progress" => {
                unsafe { std::env::set_var("OWL_PROGRESS", "1") };
            }
            // ── Native C ──────────────────────────────────────────
            // These exist so a build orchestrator can pass the project's [c]
            // section without synthesising a TOML file for the compiler to read
            // back. Repetition accumulates; there is no comma form, because a
            // cflag may legitimately contain a comma.
            "--c-source" => {
                i += 1;
                c_sources.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing path after --c-source"))?
                        .clone(),
                );
            }
            "--c-include" => {
                i += 1;
                c_includes.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing path after --c-include"))?
                        .clone(),
                );
            }
            "--cflag" => {
                i += 1;
                cflags.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing flag after --cflag"))?
                        .clone(),
                );
            }
            "--c-lib" => {
                i += 1;
                c_libs.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing library after --c-lib"))?
                        .clone(),
                );
            }
            "--define" => {
                i += 1;
                defines.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing name after --define"))?
                        .clone(),
                );
            }
            // ── Security ──────────────────────────────────────────
            "--security-mode" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing mode after --security-mode"))?;
                security_mode = Some(match value.as_str() {
                    "open" => mire::SecurityMode::Open,
                    "strict" => mire::SecurityMode::Strict,
                    other => {
                        return Err(cli_msg(&format!(
                            "Security mode must be open or strict, got '{other}'"
                        )));
                    }
                });
            }
            "--extern" => {
                i += 1;
                externs.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing pattern after --extern"))?
                        .clone(),
                );
            }
            "--extern-lib" => {
                i += 1;
                extern_libs.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing library after --extern-lib"))?
                        .clone(),
                );
            }
            "--allow-macro" => {
                i += 1;
                allow_macros.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing macro after --allow-macro"))?
                        .clone(),
                );
            }
            "--allow-unsafe" => allow_unsafe = true,
            "--macro" => {
                i += 1;
                let binding = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing name=path after --macro"))?;
                match binding.split_once('=') {
                    Some((name, path)) if !name.is_empty() && !path.is_empty() => {
                        macro_bindings.push((name.to_string(), path.to_string()));
                    }
                    _ => {
                        return Err(cli_msg(&format!(
                            "--macro expects name=path, got '{binding}'"
                        )));
                    }
                }
            }
            value if value.starts_with('-') => {
                return Err(cli_msg(&format!("Unknown option: {value}")));
            }
            value => {
                if file.is_some() {
                    return Err(cli_msg("Only one input file is supported"));
                }
                file = Some(value.to_string());
            }
        }
        i += 1;
    }

    if !matches!(mode, BuildMode::Release) && !matches!(opt_level, OptLevel::O0) {
        mode = BuildMode::Release;
    }

    let warning_filter = if show_warn {
        WarningFilter::All
    } else if !warn_codes.is_empty() {
        WarningFilter::Codes(warn_codes)
    } else {
        WarningFilter::Off
    };

    Ok((
        CommonOptions {
            mode,
            opt_level,
            output,
            output_dir,
            cache_dir,
            config,
            cache,
            lib_dir,
            artifact,
            warn: WarningCliOptions {
                filter: warning_filter,
                deny: deny_codes,
                position,
                no_warn_cats,
            },
            error_format,
            color,
            verbose,
            target,
            cpu,
            link_dirs,
            link_libs,
            ecosystem_libs,
            ecosystem_hashes,
            legacy_link_flag,
            runtime,
            bundle,
            nostartfiles,
            nostdlib,
            c_sources,
            c_includes,
            cflags,
            c_libs,
            defines,
            security_mode,
            externs,
            extern_libs,
            allow_macros,
            allow_unsafe,
            macro_bindings,
        },
        file,
    ))
}

/// Applies command-line linker/runtime overrides after the project manifest
/// has been loaded. `-L` affects native linking only; package lookup remains
/// the responsibility of Owl and `--lib-dir` remains its explicit compiler
/// escape hatch.
pub(crate) fn apply_cli_c_defs(defs: &mut mire::CDefs, options: &CommonOptions) {
    if let Some(target) = &options.target {
        defs.target = Some(target.clone());
    }
    if let Some(cpu) = &options.cpu {
        defs.cpu = Some(cpu.clone());
    }
    if let Some(runtime) = options.runtime {
        defs.runtime = runtime;
    }
    if let Some(bundle) = options.bundle {
        defs.bundle = bundle;
    }
    if options.nostartfiles {
        defs.nostartfiles = true;
    }
    if options.nostdlib {
        defs.nostdlib = true;
    }
    if let Some(artifact) = options.artifact {
        defs.artifact = artifact;
    }
    for directory in &options.link_dirs {
        let expanded = if let Some(rest) = directory.strip_prefix("~/") {
            std::env::var("HOME")
                .map(|home| format!("{home}/{rest}"))
                .unwrap_or_else(|_| directory.clone())
        } else {
            directory.clone()
        };
        defs.cflags.push(format!("-L{expanded}"));
    }
    for (_name, path) in &options.ecosystem_libs {
        let expanded = if let Some(rest) = path.strip_prefix("~/") {
            std::env::var("HOME")
                .map(|home| format!("{home}/{rest}"))
                .unwrap_or_else(|_| path.clone())
        } else {
            path.clone()
        };
        defs.cflags.push(format!("-L{expanded}"));
    }
    defs.libs.extend(options.link_libs.iter().cloned());
    // `--c-source` / `--c-lib` / `--cflag` / `--c-include` / `--define` carry the
    // project's [c] section without a TOML round trip. They extend rather than
    // replace, so an orchestrator can add a source without restating the ones
    // the manifest already had.
    defs.sources.extend(options.c_sources.iter().cloned());
    defs.libs.extend(options.c_libs.iter().cloned());
    defs.cflags.extend(options.cflags.iter().cloned());
    defs.include
        .extend(options.c_includes.iter().map(PathBuf::from));
    // CDefs has no dedicated defines list; a define is a -D flag, so that is
    // what it becomes. `--define FOO` and `--define FOO=1` both work.
    for define in &options.defines {
        if define.contains('=') {
            defs.cflags.push(format!("-D{}", define));
        } else {
            defs.cflags.push(format!("-D{}={}", define, "1"));
        }
    }
}

/// Applies the security-related command-line flags on top of whatever the
/// manifest declared. Returns the effective configuration so the caller can use
/// it without re-reading the file.
///
/// The allowlists extend the manifest's rather than replacing it: a build
/// orchestrator that forwards a project manifest plus its own additions should
/// not have to restate the project's entries.
pub(crate) fn apply_cli_security(
    security: Option<mire::SecurityConfig>,
    options: &CommonOptions,
) -> Option<mire::SecurityConfig> {
    let touches_security = options.security_mode.is_some()
        || !options.externs.is_empty()
        || !options.extern_libs.is_empty()
        || !options.allow_macros.is_empty()
        || options.allow_unsafe;
    if !touches_security {
        return security;
    }
    let mut cfg = security.unwrap_or_default();
    if let Some(mode) = options.security_mode {
        cfg.mode = mode;
    }
    for pattern in &options.externs {
        if !cfg.externs.contains(pattern) {
            cfg.externs.push(pattern.clone());
        }
    }
    for lib in &options.extern_libs {
        if !cfg.extern_libs.contains(lib) {
            cfg.extern_libs.push(lib.clone());
        }
    }
    for mac in &options.allow_macros {
        if !cfg.macros.contains(mac) {
            cfg.macros.push(mac.clone());
        }
    }
    cfg.unsafe_allowed |= options.allow_unsafe;
    Some(cfg)
}

pub(crate) fn configure_build_paths(cwd: &Path, options: &CommonOptions, source: &Path) {
    if let Some(config) = &options.config {
        let path = if config.is_absolute() {
            config.clone()
        } else {
            cwd.join(config)
        };
        unsafe { std::env::set_var("MIRE_CONFIG", path) };
    } else {
        unsafe { std::env::remove_var("MIRE_CONFIG") };
    }
    // When security flags are given on the command line, the typechecker cannot
    // read them from a TOML file (there may not be one). Publish the merged
    // configuration as JSON in the environment instead; the typechecker prefers
    // this over the file. This is the seam that lets an orchestrator drop
    // `--config` entirely.
    if options.security_mode.is_some()
        || !options.externs.is_empty()
        || !options.extern_libs.is_empty()
        || !options.allow_macros.is_empty()
        || options.allow_unsafe
    {
        let merged = apply_cli_security(None, options)
            .expect("security flags present, so a config was produced");
        if let Ok(json) = serde_json::to_string(&merged) {
            unsafe { std::env::set_var("MIRE_SECURITY", json) };
        }
    } else {
        unsafe { std::env::remove_var("MIRE_SECURITY") };
    }
    if !options.macro_bindings.is_empty() {
        // Same idea for [macros]: name=path pairs that the loader needs before
        // it can see the source.
        if let Ok(json) = serde_json::to_string(&options.macro_bindings) {
            unsafe { std::env::set_var("MIRE_MACROS", json) };
        }
    } else {
        unsafe { std::env::remove_var("MIRE_MACROS") };
    }
    if let Some(cache_dir) = &options.cache_dir {
        let path = if cache_dir.is_absolute() {
            cache_dir.clone()
        } else {
            cwd.join(cache_dir)
        };
        unsafe { std::env::set_var("MIRE_CACHE_DIR", path) };
    }
    if let Some(output_dir) = &options.output_dir {
        let path = if output_dir.is_absolute() {
            output_dir.clone()
        } else {
            cwd.join(output_dir)
        };
        unsafe { std::env::set_var("MIRE_OUTPUT_DIR", path) };
    } else {
        unsafe { std::env::remove_var("MIRE_OUTPUT_DIR") };
    }
    let _ = source;
}

pub(crate) fn parse_debug_options(cwd: &Path, args: &[String]) -> Result<DebugOptions, MireError> {
    let mut show_tokens = false;
    let mut show_ast = false;
    let mut run_binary = false;
    let mut emit_ir_only = false;
    let mut filtered = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--tokens" | "-t" => show_tokens = true,
            "--ast" | "-p" => show_ast = true,
            "--run" | "-r" => run_binary = true,
            "--ir" => emit_ir_only = true,
            _ => filtered.push(arg.clone()),
        }
    }

    let (mut common, file) = parse_common_with_file(cwd, &filtered)?;
    common.mode = BuildMode::Debug;
    if matches!(common.opt_level, OptLevel::O0) {
        common.opt_level = OptLevel::O1;
    }

    Ok(DebugOptions {
        common,
        file,
        show_tokens,
        show_ast,
        run_binary,
        emit_ir_only,
    })
}

pub(crate) fn c_defs_for(cwd: &Path, config: Option<&Path>) -> Result<mire::CDefs, MireError> {
    if let Some(config_path) = config {
        let manifest = load_config_file(&resolve_config_path(cwd, config_path))?;
        if let Some(paths) = &manifest.paths {
            if let Some(cache) = &paths.cache {
                let path = if cache.is_absolute() {
                    cache.clone()
                } else {
                    cwd.join(cache)
                };
                unsafe { std::env::set_var("MIRE_CACHE_DIR", path) };
            }
            if let Some(bin) = &paths.bin {
                let path = if bin.is_absolute() {
                    bin.clone()
                } else {
                    cwd.join(bin)
                };
                unsafe { std::env::set_var("MIRE_OUTPUT_DIR", path) };
            }
        }
        return Ok(manifest.c);
    }
    // Avenys deliberately does not discover a project manifest. Owl owns
    // project configuration and passes it through --config; direct compiler
    // use receives only the explicit CLI flags and restricted defaults.
    Ok(mire::CDefs::default())
}

pub(crate) fn resolve_config_path(cwd: &Path, config: &Path) -> PathBuf {
    if config.is_absolute() {
        config.to_path_buf()
    } else {
        cwd.join(config)
    }
}

pub(crate) fn resolve_source_path(cwd: &Path, file: Option<String>) -> Result<PathBuf, MireError> {
    let file = file.ok_or_else(|| {
        cli_msg("No input file provided; pass a .mire or .mr file (project entries are selected by Owl)")
    })?;
    let path = PathBuf::from(&file);
    let resolved = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    if !resolved.exists() {
        return Err(cli_msg(&format!(
            "Input file not found: {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

pub(crate) fn default_binary_path(source_path: &Path, mode: BuildMode) -> PathBuf {
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    default_output_dir(source_path, mode).join(stem)
}

pub(crate) fn parse_warning_code(value: &str) -> Result<DiagnosticCode, MireError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "W0001" => Ok(DiagnosticCode::W0001),
        "W0002" => Ok(DiagnosticCode::W0002),
        "W0004" => Ok(DiagnosticCode::W0004),
        "W0005" => Ok(DiagnosticCode::W0005),
        "W0006" => Ok(DiagnosticCode::W0006),
        "W0007" => Ok(DiagnosticCode::W0007),
        "W0008" => Ok(DiagnosticCode::W0008),
        "W0009" => Ok(DiagnosticCode::W0009),
        "W0010" => Ok(DiagnosticCode::W0010),
        "W0011" => Ok(DiagnosticCode::W0011),
        "W0012" => Ok(DiagnosticCode::W0012),
        "W0013" => Ok(DiagnosticCode::W0013),
        "W0014" => Ok(DiagnosticCode::W0014),
        "W0017" => Ok(DiagnosticCode::W0017),
        "W0018" => Ok(DiagnosticCode::W0018),
        "W0019" => Ok(DiagnosticCode::W0019),
        "W0021" => Ok(DiagnosticCode::W0021),
        "W0024" => Ok(DiagnosticCode::W0024),
        "W0025" => Ok(DiagnosticCode::W0025),
        "W0034" => Ok(DiagnosticCode::W0034),
        "W0035" => Ok(DiagnosticCode::W0035),
        "W0036" => Ok(DiagnosticCode::W0036),
        "W0037" => Ok(DiagnosticCode::W0037),
        "W0038" => Ok(DiagnosticCode::W0038),
        "W0039" => Ok(DiagnosticCode::W0039),
        "W0040" => Ok(DiagnosticCode::W0040),
        _ => Err(cli_msg("Warning code must look like W0001")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CommonOptions {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_run_options(Path::new("."), &owned)
            .expect("flags parse")
            .0
    }

    // ── Native C flags ───────────────────────────────────────────
    // These exist so an orchestrator can pass a project's [c] section without
    // writing a TOML for the compiler to read back.

    #[test]
    fn link_flags_separate_ecosystem_and_native_paths() {
        let dir = std::env::temp_dir();
        let dir = dir.to_str().expect("utf-8 temp dir");

        let opts = parse(&["-L", "kioto=/tmp/kioto"]);
        assert_eq!(
            opts.ecosystem_libs,
            vec![("kioto".to_string(), "/tmp/kioto".to_string())]
        );
        assert!(opts.link_dirs.is_empty());

        let opts = parse(&["--link-dir", "/tmp/native"]);
        assert_eq!(opts.link_dirs, vec!["/tmp/native".to_string()]);

        let opts = parse(&["-L", dir]);
        assert!(opts.legacy_link_flag);
        assert_eq!(opts.link_dirs, vec![dir.to_string()]);
    }

    #[test]
    fn bundle_flag_accepts_documented_modes() {
        assert_eq!(parse(&["--bundle", "static"]).bundle, Some(mire::BundleMode::Static));
        assert_eq!(
            parse(&["--bundle", "static-strict"]).bundle,
            Some(mire::BundleMode::StaticStrict)
        );
        assert_eq!(
            parse(&["--bundle", "dynamic"]).bundle,
            Some(mire::BundleMode::Dynamic)
        );
        let owned = vec!["--bundle".to_string(), "mostly".to_string()];
        assert!(parse_run_options(Path::new("."), &owned).is_err());
    }

    // `-H` applies to the most recent `--link`, so ordering is semantic rather
    // than conventional. A hash with no library to attach to must fail loudly.
    #[test]
    fn hash_attaches_to_the_preceding_link() {
        let opts = parse(&[
            "-L", "kioto=/tmp/kioto", "-H", "sha256:ab12",
            "-L", "mire=/tmp/mire",
        ]);
        assert_eq!(
            opts.ecosystem_hashes,
            vec![Some("sha256:ab12".to_string()), None]
        );

        let owned = vec!["-H".to_string(), "sha256:ab12".to_string()];
        assert!(parse_run_options(Path::new("."), &owned).is_err());

        let owned = vec![
            "-L".to_string(),
            "kioto=/tmp/kioto".to_string(),
            "-H".to_string(),
            "md5:ab12".to_string(),
        ];
        assert!(parse_run_options(Path::new("."), &owned).is_err());
    }

    #[test]
    fn output_format_flags_parse() {
        let opts = parse(&["--error-format", "json", "--color", "never"]);
        assert_eq!(opts.error_format, mire::error::format::ErrorFormat::Json);
        assert_eq!(opts.color, mire::error::format::ColorPolicy::Never);
    }

    #[test]
    fn c_sources_accumulate_across_repeats() {
        let opts = parse(&["--c-source", "a.c", "--c-source", "b.c"]);
        assert_eq!(opts.c_sources, vec!["a.c".to_string(), "b.c".to_string()]);
    }

    #[test]
    fn c_libs_and_cflags_accumulate() {
        let opts = parse(&["--c-lib", "sqlite3", "--cflag", "-O2"]);
        assert_eq!(opts.c_libs, vec!["sqlite3".to_string()]);
        assert_eq!(opts.cflags, vec!["-O2".to_string()]);
    }

    #[test]
    fn c_source_without_value_is_an_error() {
        let owned = vec!["--c-source".to_string()];
        assert!(parse_run_options(Path::new("."), &owned).is_err());
    }

    // ── Security ─────────────────────────────────────────────────

    #[test]
    fn security_mode_parses_both_modes() {
        assert_eq!(
            parse(&["--security-mode", "strict"]).security_mode,
            Some(mire::SecurityMode::Strict)
        );
        assert_eq!(
            parse(&["--security-mode", "open"]).security_mode,
            Some(mire::SecurityMode::Open)
        );
    }

    #[test]
    fn unknown_security_mode_is_rejected() {
        let owned = vec!["--security-mode".to_string(), "yolo".to_string()];
        assert!(parse_run_options(Path::new("."), &owned).is_err());
    }

    #[test]
    fn extern_patterns_accumulate() {
        let opts = parse(&["--extern", "mire_*", "--extern", "rt_*"]);
        assert_eq!(opts.externs, vec!["mire_*".to_string(), "rt_*".to_string()]);
    }

    // This is the behaviour the whole flag set exists for: a strict allowlist
    // that the caller controls without any file involved.
    #[test]
    fn strict_allowlist_is_built_from_flags_alone() {
        let opts = parse(&[
            "--security-mode",
            "strict",
            "--extern",
            "rt_*",
            "--extern-lib",
            "c",
        ]);
        let cfg = apply_cli_security(None, &opts).expect("a config was produced");
        assert_eq!(cfg.mode, mire::SecurityMode::Strict);
        assert_eq!(cfg.externs, vec!["rt_*".to_string()]);
        assert_eq!(cfg.extern_libs, vec!["c".to_string()]);
    }

    #[test]
    fn cli_extends_an_existing_allowlist_without_dropping_it() {
        let existing = mire::SecurityConfig {
            externs: vec!["rt_*".to_string()],
            ..Default::default()
        };
        let opts = parse(&["--extern", "mire_*"]);
        let cfg = apply_cli_security(Some(existing), &opts).expect("merged");
        // An orchestrator forwarding a project manifest plus its own additions
        // must not have to restate the project's entries.
        assert_eq!(cfg.externs, vec!["rt_*".to_string(), "mire_*".to_string()]);
    }

    #[test]
    fn no_security_flags_leaves_the_config_alone() {
        let existing = mire::SecurityConfig {
            mode: mire::SecurityMode::Strict,
            ..Default::default()
        };
        let opts = parse(&["--c-source", "a.c"]);
        let cfg = apply_cli_security(Some(existing), &opts).expect("passed through");
        assert_eq!(cfg.mode, mire::SecurityMode::Strict);
    }

    #[test]
    fn define_without_value_becomes_a_defined_one_flag() {
        let opts = parse(&["--define", "DEBUG", "--define", "LEVEL=3"]);
        let mut defs = mire::CDefs::default();
        apply_cli_c_defs(&mut defs, &opts);
        assert!(defs.cflags.contains(&"-DDEBUG=1".to_string()));
        assert!(defs.cflags.contains(&"-DLEVEL=3".to_string()));
    }

    #[test]
    fn macro_binding_requires_name_equals_path() {
        let opts = parse(&["--macro", "assert=path/to/assert.mire"]);
        assert_eq!(
            opts.macro_bindings,
            vec![("assert".to_string(), "path/to/assert.mire".to_string())]
        );
        let owned = vec!["--macro".to_string(), "nonsense".to_string()];
        assert!(parse_run_options(Path::new("."), &owned).is_err());
    }
}
