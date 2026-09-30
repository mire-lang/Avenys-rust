use super::build_support::{
    apply_cfg_filter, collect_used_symbols, dedup_llvm_declarations, generate_enum_constructors,
    generate_runtime_declarations, generate_struct_constructors, inject_macros,
    inject_test_harness, minimal_runtime_c_files, neutralise_ignored_tests, precompile_c_object,
    progress_phase, runtime_base, set_c_defs,
};
use super::*;
use crate::compiler::check_warnings_with_origins;
use crate::compiler::mir::{
    codegen::mir_to_llvm_with_filename, lower::lower_program_with_filename, optimize::optimize,
};
use crate::error::diagnostic::Diagnostic;
use crate::loader::load_program_with_cache;
use crate::parser::ast::Statement;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Map a target triple to the PAL platform directory name.
/// Falls back to "linux" for unknown targets (backward-compatible).
fn pal_platform_for_target(target: &str) -> &'static str {
    if target.contains("linux") {
        "pal/linux"
    } else if target.contains("darwin") || target.contains("apple") {
        "pal/darwin"
    } else if target.contains("windows") || target.contains("mingw") {
        "pal/windows"
    } else if target.contains("freebsd") {
        "pal/freebsd"
    } else {
        "pal/linux"
    }
}

pub fn compile_file_with_avenys(source_path: &Path, options: &BuildOptions) -> Result<BuildResult> {
    let source = fs::read_to_string(source_path).map_err(|err| {
        crate::error::MireError::runtime(format!(
            "Could not read '{}': {}",
            source_path.display(),
            err
        ))
    })?;
    let source_filename = source_path.display().to_string();
    let normalized = normalize_test_build_options(options);
    let options = normalized.as_ref().unwrap_or(options);
    match compile_file_inner(source_path, options, &source, &source_filename) {
        Ok(result) => Ok(result),
        Err(err) => Err(err.ensure_context(&source_filename, &source)),
    }
}

/// Enforce what a test build needs, independently of what the project builds.
///
/// A test always needs an executable, and always needs the runtime to run on.
/// What a project publishes is irrelevant to both: a library configured as
/// `shared` would otherwise build a shared object with no test entry point, and
/// the runner would report every test file as ok having run no assertion. This
/// runs once, at the entry point, before any consumer of `artifact` or `runtime`
/// branches on them, so the test path cannot be steered by a manifest.
///
/// Returns `None` when the options already satisfy both, so the common case
/// borrows rather than clones.
fn normalize_test_build_options(options: &BuildOptions) -> Option<BuildOptions> {
    if !options.test_mode {
        return None;
    }
    let needs_artifact = !matches!(options.c_defs.artifact, LibType::Bin);
    let needs_runtime = matches!(options.c_defs.runtime, RuntimeTier::None);
    if !needs_artifact && !needs_runtime {
        return None;
    }
    Some(BuildOptions {
        c_defs: CDefs {
            artifact: LibType::Bin,
            runtime: match options.c_defs.runtime {
                RuntimeTier::None => RuntimeTier::Minimal,
                tier => tier,
            },
            ..options.c_defs.clone()
        },
        ..options.clone()
    })
}

fn compile_file_inner(
    source_path: &Path,
    options: &BuildOptions,
    source: &str,
    source_filename: &str,
) -> Result<BuildResult> {
    let build_start = std::time::Instant::now();
    let output_dir = default_output_dir(source_path, options.mode);
    fs::create_dir_all(&output_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!(
                "Could not create build directory '{}': {}",
                output_dir.display(),
                err
            ),
        })
    })?;

    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    // For libraries, use project name with proper prefix/suffix
    let binary_path = if matches!(options.c_defs.artifact, LibType::Static | LibType::Shared) {
        // Try to get project name from manifest
        let project_name = find_project_root(source_path)
            .and_then(|root| load_project_manifest(&root).ok().flatten())
            .map(|m| m.project.name)
            .unwrap_or_else(|| stem.to_string());
        let (prefix, suffix) = match options.c_defs.artifact {
            LibType::Static => ("lib", ".a"),
            LibType::Shared => ("lib", ".so"),
            _ => ("", ""),
        };
        options
            .output
            .clone()
            .unwrap_or_else(|| output_dir.join(format!("{prefix}{project_name}{suffix}")))
    } else {
        options
            .output
            .clone()
            .unwrap_or_else(|| output_dir.join(stem))
    };
    let ir_path = options
        .persist_ir
        .then(|| output_dir.join(format!("{stem}.ll")));
    let optimized_ir_path = options
        .persist_ir
        .then(|| output_dir.join(format!("{stem}.opt.ll")));
    let runtime_base = runtime_base();
    set_c_defs(options.c_defs.clone());
    let (mut c_source_files, c_sources_hash) = if options.emit_binary {
        let mut files = Vec::new();
        // Tier-aware C file collection:
        //   full:    compile all runtime + PAL C sources (backward-compatible)
        //   minimal: compile PAL C sources only (runtime is demand-driven at IR level)
        //   none:    compile no runtime/PAL C sources (freestanding; user provides their own)
        let runtime_tier = options.c_defs.runtime;
        if !matches!(runtime_tier, RuntimeTier::None) {
            if matches!(runtime_tier, RuntimeTier::Full) {
                // Full tier: compile all runtime and PAL sources (exclude _minimal.c variants)
                let pal_platform = pal_platform_for_target(
                    options
                        .c_defs
                        .target
                        .as_deref()
                        .unwrap_or("x86_64-unknown-linux-gnu"),
                );
                for directory in ["runtime", "pal/core"]
                    .iter()
                    .chain(std::iter::once(&pal_platform))
                {
                    let dir = runtime_base.join(directory);
                    for entry in std::fs::read_dir(&dir)
                        .map_err(|err| {
                            MireError::new(ErrorKind::Runtime {
                                span: crate::error::Span::unknown(),
                                message: format!(
                                    "Could not read C sources from {directory}: {err}"
                                ),
                            })
                        })?
                        .flatten()
                    {
                        let path = entry.path();
                        if path.extension().is_some_and(|e| e == "c") {
                            let fname = path.file_name().unwrap().to_string_lossy();
                            // Skip _minimal.c variants in full tier (they're for minimal tier only)
                            if !fname.ends_with("_minimal.c") {
                                files.push(path.to_string_lossy().into_owned());
                            }
                        }
                    }
                }
            } else {
                // Minimal tier: collect only runtime sources (PAL added on demand).
                // After IR generation, R3.2 filters runtime .c to only needed files,
                // and adds PAL .c files only if the program uses PAL symbols.
                // The full hash is kept for conservative cache invalidation.
                let dir = runtime_base.join("runtime");
                for entry in std::fs::read_dir(&dir)
                    .map_err(|err| {
                        MireError::new(ErrorKind::Runtime {
                            span: crate::error::Span::unknown(),
                            message: format!("Could not read C sources from runtime: {err}"),
                        })
                    })?
                    .flatten()
                {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "c") {
                        let fname = path.file_name().unwrap().to_string_lossy();
                        // In minimal tier, prefer _minimal.c variants, but also include base files
                        // that don't have a _minimal counterpart
                        if fname.ends_with("_minimal.c")
                            || !files
                                .iter()
                                .any(|f| f.contains(&fname.replace("_minimal", "")))
                        {
                            files.push(path.to_string_lossy().into_owned());
                        }
                    }
                }
            }
            // Also collect C sources from the compiler's runtime directory (standard library)
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let compiler_runtime = manifest_dir.join("src/runtime");
            if compiler_runtime.exists() {
                for entry in std::fs::read_dir(&compiler_runtime)
                    .map_err(|err| {
                        MireError::new(ErrorKind::Runtime {
                            span: crate::error::Span::unknown(),
                            message: format!(
                                "Could not read C sources from compiler runtime: {err}"
                            ),
                        })
                    })?
                    .flatten()
                {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "c") {
                        let fname = path.file_name().unwrap().to_string_lossy();
                        if matches!(runtime_tier, RuntimeTier::Full) {
                            if !fname.ends_with("_minimal.c") {
                                files.push(path.to_string_lossy().into_owned());
                            }
                        } else {
                            if fname.ends_with("_minimal.c")
                                || !files
                                    .iter()
                                    .any(|f| f.contains(&fname.replace("_minimal", "")))
                            {
                                files.push(path.to_string_lossy().into_owned());
                            }
                        }
                    }
                }
            }
        }
        for proj_src in &options.c_defs.sources {
            let root = crate::avens::manifest::find_project_root(source_path)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let p = if std::path::Path::new(proj_src).is_absolute() {
                std::path::PathBuf::from(proj_src)
            } else {
                root.join(proj_src)
            };
            if p.exists() {
                files.push(p.to_string_lossy().into_owned());
            } else {
                return Err(MireError::new(ErrorKind::Runtime {
                    span: crate::error::Span::unknown(),
                    message: format!("C source '{}' declared in [c] was not found", p.display()),
                }));
            }
        }
        files.sort();
        files.dedup();
        let hash = {
            let mut hasher = crate::incremental::FxHasher::new();
            for src in &files {
                if let Ok(meta) = fs::metadata(src) {
                    meta.len().hash(&mut hasher);
                    if let Ok(mtime) = meta.modified() {
                        mtime.hash(&mut hasher);
                    }
                }
            }
            hasher.finish()
        };
        (files, hash)
    } else {
        (Vec::new(), 0)
    };
    let cache_settings = CacheSettings::resolve_for(source_path, options.cache)?;
    let mut cache = IncrementalCache::load_with_settings(source_path, cache_settings)?;
    let mut loaded = load_program_with_cache(source_path, &mut cache, options.import_mode)?;
    // An ignored test must not be compiled either, or a test quarantined for a
    // compile error still fails the build through its own body. This has to
    // happen here, before analysis, because the harness that skips the call is
    // injected into the codegen copy further down. Gated on test mode so a
    // normal build of the same file keeps the function intact.
    if options.test_mode {
        neutralise_ignored_tests(&mut loaded.program);
    }
    let phase_load = build_start.elapsed().as_millis() as u64;
    progress_phase("load", source_filename, phase_load, phase_load);
    let source_file_hash = source_hash(source);
    let dep_fingerprint = dependency_fingerprint(&loaded.files);
    if options.debug_dump
        && let Some(report) =
            cache.analysis_invalidation_report(source_path, source_file_hash, &loaded.program)
    {
        eprintln!(
            "[AVENYS][incremental] changed_units={} invalidated_units={} added_units={} removed_units={}",
            report.changed_units.len(),
            report.invalidated_units.len(),
            report.added_units.len(),
            report.removed_units.len(),
        );
    }
    let fingerprint = build_fingerprint(
        source_path,
        &loaded.files,
        options.mode,
        options.import_mode,
        options.opt_level,
        options.emit_binary,
        &format!(
            "{:x}|artifact={:?}|runtime={:?}|target={:?}|nostartfiles={}|nostdlib={}|cflags={:?}|libs={:?}",
            c_sources_hash,
            options.c_defs.artifact,
            options.c_defs.runtime,
            options.c_defs.target,
            options.c_defs.nostartfiles,
            options.c_defs.nostdlib,
            options.c_defs.cflags,
            options.c_defs.libs,
        ),
    );

    if let Some(entry) = cache.build_entry(
        source_path,
        options.mode,
        options.import_mode,
        options.emit_binary,
        options.persist_ir,
        options.test_mode,
    ) && entry.fingerprint == fingerprint
        && (!options.emit_binary || entry.binary_path.exists())
        && entry.binary_path == binary_path
        && entry.ir_path == ir_path
        && entry.optimized_ir_path == optimized_ir_path
        && entry.ir_path.as_ref().is_none_or(|path| path.exists())
        && entry
            .optimized_ir_path
            .as_ref()
            .is_none_or(|path| path.exists())
    {
        cache.record_build_hit();
        if options.debug_dump {
            let metrics = cache.metrics();
            eprintln!(
                "[AVENYS][incremental] cache_metrics file_hit={} file_miss={} analysis_hit={} analysis_miss={} build_hit={} build_miss={} evictions={}",
                metrics.file_hits,
                metrics.file_misses,
                metrics.analysis_hits,
                metrics.analysis_misses,
                metrics.build_hits,
                metrics.build_misses,
                metrics.evictions,
            );
        }
        return Ok(BuildResult {
            binary_path,
            ir_path,
            optimized_ir_path,
            used_optimizations: !matches!(options.opt_level, OptLevel::O0),
            warnings: Vec::new(),
            warnings_raw: Vec::new(),
        });
    }
    cache.record_build_miss();

    let mut phase_analyse_time = phase_load;
    let mut phase_mir_time = phase_load;
    let program = if let Some(cached) =
        cache.cached_analysis(source_path, source_file_hash, dep_fingerprint)
    {
        match cached {
            CachedAnalysis::Success(mut program) => {
                apply_cfg_filter(&mut program);
                // Macro injection is part of the effective program, not just
                // a first-build preprocessing step. Reapply it after loading
                // cached analysis so newly added or changed library macros
                // cannot disappear from incremental builds.
                inject_macros(&mut program, source_path);
                program
            }
            CachedAnalysis::Error(error) => return Err(error),
        }
    } else {
        let mut program = loaded.program;
        apply_cfg_filter(&mut program);
        inject_macros(&mut program, source_path);
        let analysis_result = if let Some(cached) =
            cache.latest_successful_analysis(source_path, source_file_hash)
        {
            let (selection, _) = prepare_program_with_partial_analysis_reuse(&mut program, cached);
            if selection
                .statement_mask
                .iter()
                .all(|should_check| !should_check)
            {
                Ok(())
            } else {
                analyze_program_with_origins_partial(
                    &mut program,
                    source,
                    &loaded.statement_origins,
                    &loaded.sources,
                    &selection,
                )
                .map(|_| ())
            }
        } else {
            analyze_program_with_origins(
                &mut program,
                source,
                &loaded.statement_origins,
                &loaded.sources,
            )
            .map(|_| ())
        };

        if let Err(err) = analysis_result {
            let err = if err.source().is_none() {
                err.with_source(source.to_string())
            } else {
                err
            };
            let err = if err.filename().is_none() {
                err.with_filename(source_filename.to_string())
            } else {
                err
            };
            cache.store_analysis_error(
                source_path,
                source_file_hash,
                dep_fingerprint,
                &program,
                &err,
            )?;
            cache.save()?;
            return Err(err);
        }
        cache.store_analysis(source_path, source_file_hash, dep_fingerprint, &program)?;
        phase_analyse_time = build_start.elapsed().as_millis() as u64;
        progress_phase(
            "analyse",
            source_filename,
            phase_analyse_time - phase_load,
            phase_analyse_time,
        );
        program
    };

    // The test harness replaces the user's `main` with a runner. It is applied
    // only to the codegen copy AFTER analysis so the persisted analysis cache
    // entry always holds the clean program: a test-mode build stores clean
    // analysis, and a later `mire run`/`owl run` (same analysis key) must load
    // the user's real `main`, never a baked-in test runner.
    let mut program = program;
    if options.test_mode {
        inject_test_harness(&mut program);
    }

    let warnings = check_warnings_with_origins(
        &program,
        source,
        Some(source_filename),
        options.warning_filter.clone(),
        options.deny_warnings.clone(),
        &loaded.statement_origins,
        source_path,
    );
    let mut warning_strs = Vec::new();
    let warnings_raw: Vec<Diagnostic> = warnings.clone();
    for diagnostic in &warnings {
        warning_strs.push(format_diagnostic(diagnostic, true));
    }
    if let Some(err_diag) = warnings
        .iter()
        .find(|d| matches!(d.severity, Severity::Error))
    {
        return Err(MireError::from_diagnostic(err_diag));
    }

    let (mut ir, extern_libs) = {
        let mut mir = lower_program_with_filename(&program, source_filename);

        // Pre-codegen validation: surface calls to symbols the backend cannot
        // resolve (stale bare builtins, removed PAL functions, or unloaded
        // externs) at their source location instead of letting LLVM-opt fail
        // later with an opaque `use of undefined value '@x'` and
        // `<no source location available>`.
        if let Some((bad_name, (line, col))) =
            crate::compiler::mir::codegen::find_first_undefined_call(&mir)
        {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::new(line, col),
                message: format!(
                    "undefined function '{}': cannot resolve a codegen target for this \
                     call (the symbol is not loaded, was removed, or uses a bare name the \
                     compiler no longer emits). Use the namespaced form, e.g. `fs::exists`.",
                    bad_name
                ),
            }));
        }

        // Compute combined hash of all MIR function bodies for caching
        let mir_hash: u64 = {
            let mut hasher = crate::incremental::FxHasher::new();
            for func in &mir.functions {
                hasher.write_u64(func.body_hash);
            }
            Hasher::finish(&hasher)
        };

        // Check MIR program cache
        let cached_program_ir =
            cache.get_cached_mir_fn(source_path, "_program", mir_hash, options.opt_level);

        if let Some(cached_ir) = cached_program_ir {
            if options.debug_dump {
                eprintln!(
                    "[MIR] program cache hit ({} functions)",
                    mir.functions.len()
                );
            }
            (cached_ir, mir.extern_libs.clone())
        } else {
            let opt_count = optimize(&mut mir);
            if options.debug_dump && opt_count > 0 {
                eprintln!("[MIR] applied {} optimizations", opt_count);
            }
            if options.debug_dump && mir.functions.iter().any(|f| f.name.contains("complex")) {
                for f in &mir.functions {
                    eprintln!("[MIR] function: {} ({} blocks)", f.name, f.blocks.len());
                    for b in &f.blocks {
                        eprintln!("  block {} ({}):", b.id, b.label);
                        for inst in &b.insts {
                            eprintln!("    {:?} -> {:?}", inst.result, inst.op);
                        }
                        eprintln!("    term: {:?}", b.terminator);
                    }
                }
            }
            let (ir, extern_libs) = mir_to_llvm_with_filename(&mir, source_filename);
            phase_mir_time = build_start.elapsed().as_millis() as u64;
            progress_phase(
                "mir",
                source_filename,
                phase_mir_time - phase_analyse_time,
                phase_mir_time,
            );
            if let Err(e) =
                cache.store_cached_mir_fn(source_path, "_program", mir_hash, options.opt_level, &ir)
                && options.debug_dump
            {
                eprintln!("[MIR] cache store error: {}", e);
            }
            (ir, extern_libs)
        }
    };
    // Scan IR for used symbols (used for both tier enforcement and selective .c compilation).
    let used = collect_used_symbols(&ir);
    // Enforce runtime tier contract: runtime="none" means no rt_* calls are allowed.
    if matches!(options.c_defs.runtime, RuntimeTier::None) && !used.runtime.is_empty() {
        let mut symbols: Vec<&str> = used.runtime.iter().map(|s| s.as_str()).collect();
        symbols.sort();
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!(
                "runtime = \"none\" but program uses runtime symbols: {}. \
                 Set [c] runtime = \"minimal\" or \"full\", or remove these dependencies.",
                symbols.join(", ")
            ),
        }));
    }
    // R3.2 — Selective .c compilation for minimal tier:
    // After IR generation, we know which runtime and PAL symbols are actually used.
    // Filter runtime c_source_files to only needed files, and add PAL files on demand.
    // The c_sources_hash was computed from ALL runtime files (conservative fingerprint).
    if matches!(options.c_defs.runtime, RuntimeTier::Minimal) && !c_source_files.is_empty() {
        let needed = minimal_runtime_c_files(&used.runtime);
        let before = c_source_files.len();
        // Filter: keep only runtime files that are needed
        c_source_files.retain(|path| {
            let fname = std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            needed.contains(&fname)
        });
        // Add PAL files on demand: when the program's IR uses PAL symbols OR a
        // retained runtime .c file bridges into PAL at the C level (helpers.c,
        // thread.c call pal_* directly, invisible to the IR symbol scan).
        let runtime_needs_pal = c_source_files.iter().any(|path| {
            let fname = std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            matches!(fname.as_ref(), "helpers.c" | "thread.c")
        });
        if !used.pal.is_empty() || runtime_needs_pal {
            let pal_platform = pal_platform_for_target(
                options
                    .c_defs
                    .target
                    .as_deref()
                    .unwrap_or("x86_64-unknown-linux-gnu"),
            );
            let pal_dirs: Vec<&str> = ["pal/core"]
                .iter()
                .chain(std::iter::once(&pal_platform))
                .copied()
                .collect();
            for directory in &pal_dirs {
                let dir = runtime_base.join(directory);
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().is_some_and(|e| e == "c") {
                            c_source_files.push(p.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        // Always keep user-declared [c] sources
        for proj_src in &options.c_defs.sources {
            let root = crate::avens::manifest::find_project_root(source_path)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let p = if std::path::Path::new(proj_src).is_absolute() {
                std::path::PathBuf::from(proj_src)
            } else {
                root.join(proj_src)
            };
            if p.exists() {
                let path_str = p.to_string_lossy().into_owned();
                if !c_source_files.contains(&path_str) {
                    c_source_files.push(path_str);
                }
            }
        }
        c_source_files.sort();
        c_source_files.dedup();
        if options.debug_dump {
            eprintln!(
                "[R3.2] minimal tier: {} → {} .c files ({} used rt symbols, {} used pal symbols)",
                before,
                c_source_files.len(),
                used.runtime.len(),
                used.pal.len(),
            );
        }
    }
    // Append runtime declarations and struct constructor functions (MIR codegen path)
    {
        let runtime_decls = generate_runtime_declarations(&ir);
        if !runtime_decls.is_empty() {
            if let Some(pos) = ir.find('\n') {
                ir.insert_str(pos + 1, &runtime_decls);
            } else {
                ir.push('\n');
                ir.push_str(&runtime_decls);
            }
        }
        let needs_ctors = program.statements.iter().any(|stmt| {
            if let Statement::Type { name, .. } = stmt {
                !ir.contains(&format!("define ptr @{}(", name))
            } else {
                false
            }
        });
        if needs_ctors {
            let struct_ctors = generate_struct_constructors(&program);
            if !struct_ctors.is_empty() {
                ir.push('\n');
                ir.push_str(&struct_ctors);
            }
        }
        let needs_enum_ctors = program.statements.iter().any(|stmt| {
            if let Statement::Enum { name, variants, .. } = stmt {
                variants
                    .iter()
                    .any(|v| !ir.contains(&format!("define ptr @{}.{}(", name, v.name)))
            } else {
                false
            }
        });
        if needs_enum_ctors {
            let enum_ctors = generate_enum_constructors(&program);
            if !enum_ctors.is_empty() {
                ir.push('\n');
                ir.push_str(&enum_ctors);
            }
        }
        // Add @main entry point wrapper if the program defines @fn_main
        // Respect @[no_main] attribute to skip wrapper generation (freestanding mode)
        let has_no_main = program.file_attributes.iter().any(|a| a.name == "no_main")
            || program.statements.iter().any(|s| {
                if let Statement::Function { attributes, .. } = s {
                    attributes.iter().any(|a| a.name == "no_main")
                } else {
                    false
                }
            });
        if !has_no_main
            && ir.contains("define")
            && ir.contains("@fn_main")
            && !ir.contains("define i32 @main(")
        {
            // `fn_main` is `void` for a unit `main` and `i64` for one that
            // returns a status. Reading a return value out of a `void` function
            // is undefined behaviour: the wrapper used to do it unconditionally
            // and the process then exited with whatever happened to be left in
            // the return register, so a program whose last call was e.g.
            // `proc::run_output` exited non-zero despite succeeding.
            let main_returns_void = ir
                .lines()
                .find(|line| line.contains("define") && line.contains("@fn_main("))
                .map(|line| line.split_whitespace().nth(1) == Some("void"))
                .unwrap_or(false);
            ir.push_str("\n\ndefine i32 @main(i32 %argc, ptr %argv) {\n");
            ir.push_str("  store i32 %argc, ptr @.argc\n");
            ir.push_str("  store ptr %argv, ptr @.argv\n");
            if main_returns_void {
                ir.push_str("  call void @fn_main(ptr null)\n");
                ir.push_str("  ret i32 0\n");
            } else {
                ir.push_str("  %call_main = call i64 @fn_main(ptr null)\n");
                ir.push_str("  %exit_code = trunc i64 %call_main to i32\n");
                ir.push_str("  ret i32 %exit_code\n");
            }
            ir.push_str("}\n");
        }
        ir = dedup_llvm_declarations(&ir);
    }
    if let Some(path) = &ir_path {
        fs::write(path, &ir).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Could not write '{}': {}", path.display(), err),
            })
        })?;
    }
    let final_ir = if matches!(options.opt_level, OptLevel::O0) {
        ir
    } else {
        let _ = fs::write("/tmp/opencode/preopt.ll", &ir);
        optimize_ir(&ir, options.opt_level, source_filename)?
    };
    let phase_llvm = build_start.elapsed().as_millis() as u64;
    progress_phase(
        "llvm",
        source_filename,
        phase_llvm - phase_mir_time,
        phase_llvm,
    );

    if let Some(path) = &optimized_ir_path {
        fs::write(path, &final_ir).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Could not write '{}': {}", path.display(), err),
            })
        })?;
    }

    if options.emit_binary {
        let cache_dir = std::env::var_os("MIRE_CACHE_DIR")
            .map(PathBuf::from)
            .map(|path| path.join("cobjects"))
            .unwrap_or_else(|| runtime_base.join(".cobject_cache"));
        let cache_dir = if fs::create_dir_all(&cache_dir).is_ok() {
            cache_dir
        } else {
            let fallback = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".cobject_cache");
            let _ = fs::create_dir_all(&fallback);
            fallback
        };
        let c_objects: Vec<String> = if c_source_files.len() <= 1 {
            c_source_files
                .iter()
                .map(|src| precompile_c_object(src, &cache_dir, &runtime_base))
                .collect::<Result<_>>()?
        } else {
            let results = std::sync::Mutex::new(vec![String::new(); c_source_files.len()]);
            std::thread::scope(|s| {
                for (i, src) in c_source_files.iter().enumerate() {
                    let results = &results;
                    let cache_dir = &cache_dir;
                    let runtime_base = &runtime_base;
                    s.spawn(
                        move || match precompile_c_object(src, cache_dir, runtime_base) {
                            Ok(obj) => {
                                results.lock().unwrap()[i] = obj;
                            }
                            Err(e) => {
                                let mut results = results.lock().unwrap();
                                results[i] = format!("<{}: {}>", src, e);
                            }
                        },
                    );
                }
            });
            let results = results.into_inner().unwrap();
            if results.iter().any(|s| s.is_empty() || s.starts_with('<')) {
                let failures: Vec<&str> = results
                    .iter()
                    .filter(|s| s.is_empty() || s.starts_with('<'))
                    .map(|s| s.as_str())
                    .collect();
                return Err(MireError::runtime(format!(
                    "C object compilation failed for: {}",
                    failures.join(", ")
                )));
            }
            results
        };
        let has_pal_objects = c_source_files.iter().any(|f| f.contains("pal/"));
        let needs_sodium = has_pal_objects
            || used
                .runtime
                .iter()
                .any(|symbol| symbol.starts_with("rt_crypto_"));
        compile_binary_from_ir(
            &final_ir,
            &c_objects,
            &binary_path,
            &extern_libs,
            options.opt_level,
            source_filename,
            needs_sodium,
        )?;
        let phase_link = build_start.elapsed().as_millis() as u64;
        progress_phase("link", source_filename, phase_link - phase_llvm, phase_link);
    }
    let phase_done = build_start.elapsed().as_millis() as u64;
    progress_phase("done", source_filename, 0, phase_done);

    cache.store_build(
        source_path,
        BuildCacheEntry {
            fingerprint,
            mode: options.mode,
            import_mode: options.import_mode,
            opt_level: options.opt_level,
            emit_binary: options.emit_binary,
            persist_ir: options.persist_ir,
            binary_path: binary_path.clone(),
            ir_path: ir_path.clone(),
            optimized_ir_path: optimized_ir_path.clone(),
        },
        options.test_mode,
    );
    if options.debug_dump {
        let metrics = cache.metrics();
        eprintln!(
            "[AVENYS][incremental] cache_metrics file_hit={} file_miss={} analysis_hit={} analysis_miss={} build_hit={} build_miss={} evictions={}",
            metrics.file_hits,
            metrics.file_misses,
            metrics.analysis_hits,
            metrics.analysis_misses,
            metrics.build_hits,
            metrics.build_misses,
            metrics.evictions,
        );
    }
    cache.save()?;

    Ok(BuildResult {
        binary_path,
        ir_path,
        optimized_ir_path,
        used_optimizations: !matches!(options.opt_level, OptLevel::O0),
        warnings: warning_strs,
        warnings_raw,
    })
}

pub fn default_output_dir(source_path: &Path, mode: BuildMode) -> PathBuf {
    // Owl's normalized config supplies an exact output directory. It must win
    // over project auto-discovery so managed builds are reproducible.
    if let Some(output_dir) = std::env::var_os("MIRE_OUTPUT_DIR") {
        return PathBuf::from(output_dir);
    }
    if let Some(project_root) =
        find_project_root(source_path.parent().unwrap_or_else(|| Path::new(".")))
    {
        return project_root.join("bin").join(match mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        });
    }

    source_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("bin")
        .join(match mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(artifact: LibType, runtime: RuntimeTier, test_mode: bool) -> BuildOptions {
        BuildOptions {
            c_defs: CDefs {
                artifact,
                runtime,
                ..CDefs::default()
            },
            test_mode,
            ..BuildOptions::default()
        }
    }

    #[test]
    fn a_test_build_is_artifact_independent() {
        // The library case that made `owl test` report every file as ok having
        // run nothing: kioto, mire, sdl, sqlite and blu all declare this.
        let shared = normalize_test_build_options(&opts(LibType::Shared, RuntimeTier::Minimal, true))
            .expect("shared artifact must be normalized for a test");
        assert!(matches!(shared.c_defs.artifact, LibType::Bin));

        let statik =
            normalize_test_build_options(&opts(LibType::Static, RuntimeTier::Minimal, true))
                .expect("static artifact must be normalized for a test");
        assert!(matches!(statik.c_defs.artifact, LibType::Bin));

        // Already an executable: nothing to do, so nothing is cloned.
        assert!(normalize_test_build_options(&opts(LibType::Bin, RuntimeTier::Minimal, true))
            .is_none());
    }

    #[test]
    fn a_test_build_always_gets_a_runtime() {
        // A test needs the runtime to run on, so a manifest declaring the
        // freestanding tier is lifted to minimal.
        let freestanding =
            normalize_test_build_options(&opts(LibType::Bin, RuntimeTier::None, true))
                .expect("none runtime must be lifted for a test");
        assert!(matches!(freestanding.c_defs.runtime, RuntimeTier::Minimal));

        // A full-tier project keeps its tier. Paired with a shared artifact so
        // there is a normalization to observe: only the artifact should move.
        let full = normalize_test_build_options(&opts(LibType::Shared, RuntimeTier::Full, true))
            .expect("shared artifact must be normalized alongside a full tier");
        assert!(matches!(full.c_defs.runtime, RuntimeTier::Full));
        assert!(matches!(full.c_defs.artifact, LibType::Bin));

        // Bin + full needs no normalization at all, and must not clone to say so.
        assert!(normalize_test_build_options(&opts(LibType::Bin, RuntimeTier::Full, true)).is_none());
    }

    #[test]
    fn a_normal_build_is_never_touched() {
        // The whole point is that this applies to tests only. A library build
        // must keep the artifact it was configured with, or packages would
        // stop being able to publish.
        assert!(normalize_test_build_options(&opts(LibType::Shared, RuntimeTier::None, false))
            .is_none());
        assert!(normalize_test_build_options(&opts(LibType::Static, RuntimeTier::Minimal, false))
            .is_none());
    }
}
