use crate::cli::*;
use mire::{
    BuildMode, CacheOverrides, MireError, OptLevel, default_output_dir, find_project_root,
    load_project_manifest,
};
use mire::error::diagnostic::{DiagnosticCode, WarningFilter};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct CommonOptions {
    pub(crate) mode: BuildMode,
    pub(crate) opt_level: OptLevel,
    pub(crate) output: Option<PathBuf>,
    pub(crate) cache: CacheOverrides,
    pub(crate) owl_home: Option<PathBuf>,
    pub(crate) warn: WarningCliOptions,
    pub(crate) verbose: bool,
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
    cwd: &Path,
    args: &[String],
) -> Result<(CommonOptions, Option<String>), MireError> {
    let mut mode = BuildMode::Debug;
    let mut opt_level = OptLevel::O0;
    let mut output = None;
    let mut file = None;
    let mut cache = CacheOverrides::default();
    let mut owl_home = None;
    let mut verbose = false;
    let mut show_warn = false;
    let mut position = false;
    let mut warn_codes = HashSet::new();
    let mut deny_codes = HashSet::new();
    let mut no_warn_cats: Vec<String> = Vec::new();

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
                let level = args.get(i).ok_or_else(|| {
                    runtime_msg("Missing optimization level after -O/--opt-level")
                })?;
                opt_level = OptLevel::parse(level)
                    .ok_or_else(|| runtime_msg("Invalid optimization level, use 0/1/2/3/s/z"))?;
            }
            flag if flag.starts_with("-O") && flag.len() > 2 => {
                opt_level = OptLevel::parse(&flag[2..])
                    .ok_or_else(|| runtime_msg("Invalid optimization level, use 0/1/2/3/s/z"))?;
            }
            "-o" | "--output" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| runtime_msg("Missing output path after -o/--output"))?;
                output = Some(PathBuf::from(value));
            }
            "--owl-home" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| runtime_msg("Missing value for --owl-home"))?;
                owl_home = Some(PathBuf::from(value));
            }
            "--cache-max-units" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| runtime_msg("Missing value for --cache-max-units"))?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| runtime_msg("Invalid --cache-max-units value"))?;
                cache.max_units = Some(parsed);
            }
            "--no-analysis-cache" => cache.analysis_cache = Some(false),
            "--analysis-cache" => cache.analysis_cache = Some(true),
            "--show-warn" | "--sh-warn" => show_warn = true,
            "--position" | "--pos" => position = true,
            "--warnings-as-errors" | "--deny-warnings" => {
                for code in [DiagnosticCode::W0001, DiagnosticCode::W0002, DiagnosticCode::W0004, DiagnosticCode::W0005, DiagnosticCode::W0034, DiagnosticCode::W0039] {
                    deny_codes.insert(code);
                }
            }
            "--no-warn" => {
                i += 1;
                let cat = args.get(i).ok_or_else(|| runtime_msg("Missing warning category after --no-warn"))?;
                no_warn_cats.push(cat.clone());
            }
            "-W" => {
                i += 1;
                let code = args
                    .get(i)
                    .ok_or_else(|| runtime_msg("Missing warning code after -W"))?;
                warn_codes.insert(parse_warning_code(code)?);
            }
            "--deny" => {
                i += 1;
                let code = args
                    .get(i)
                    .ok_or_else(|| runtime_msg("Missing warning code after --deny"))?;
                deny_codes.insert(parse_warning_code(code)?);
            }
            "--verbose" | "-v" => verbose = true,
            "--progress" => {
                unsafe { std::env::set_var("OWL_PROGRESS", "1") };
            }
            value if value.starts_with('-') => {
                return Err(runtime_msg(&format!("Unknown option: {value}")));
            }
            value => {
                if file.is_some() {
                    return Err(runtime_msg("Only one input file is supported"));
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

    if file.is_none() {
        file = default_entry_from_manifest(cwd)?;
    }

    Ok((
        CommonOptions {
            mode,
            opt_level,
            output,
            cache,
            owl_home,
            warn: WarningCliOptions {
                filter: warning_filter,
                deny: deny_codes,
                position,
                no_warn_cats,
            },
            verbose,
        },
        file,
    ))
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

pub(crate) fn default_entry_from_manifest(cwd: &Path) -> Result<Option<String>, MireError> {
    let project_root = match find_project_root(cwd) {
        Some(root) => root,
        None => return Ok(None),
    };
    let manifest = load_project_manifest(&project_root)?;
    let entry = manifest.map(|m| m.project.entry).unwrap_or_default();
    let path = project_root.join(&entry);
    Ok(Some(path.to_string_lossy().to_string()))
}

pub(crate) fn resolve_source_path(cwd: &Path, file: Option<String>) -> Result<PathBuf, MireError> {
    let file = file.ok_or_else(|| {
        runtime_msg("No input file provided and no `entry` was found in owl.toml")
    })?;
    let path = PathBuf::from(&file);
    let resolved = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    if !resolved.exists() {
        return Err(runtime_msg(&format!(
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
        _ => Err(runtime_msg("Warning code must look like W0001")),
    }
}
