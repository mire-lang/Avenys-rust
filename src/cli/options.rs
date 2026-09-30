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
    pub(crate) verbose: bool,
    pub(crate) target: Option<String>,
    pub(crate) link_dirs: Vec<String>,
    pub(crate) link_libs: Vec<String>,
    pub(crate) runtime: Option<mire::RuntimeTier>,
    pub(crate) nostartfiles: bool,
    pub(crate) nostdlib: bool,
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
    let mut link_dirs = Vec::new();
    let mut link_libs = Vec::new();
    let mut runtime = None;
    let mut nostartfiles = false;
    let mut nostdlib = false;

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
            "-L" | "--link" => {
                i += 1;
                link_dirs.push(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing path after -L/--link"))?
                        .clone(),
                );
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
                        .ok_or_else(|| cli_msg("Runtime must be full, minimal, or none"))?,
                );
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
            verbose,
            target,
            link_dirs,
            link_libs,
            runtime,
            nostartfiles,
            nostdlib,
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
    if let Some(runtime) = options.runtime {
        defs.runtime = runtime;
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
    defs.libs.extend(options.link_libs.iter().cloned());
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
