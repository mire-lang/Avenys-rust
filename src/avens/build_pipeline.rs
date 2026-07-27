use super::*;
use crate::compiler::check_warnings_with_origins;
use crate::compiler::mir::{codegen::mir_to_llvm_with_filename, lower::lower_program_with_filename, optimize::optimize};
use crate::error::diagnostic::Diagnostic;
use crate::loader::load_program_with_cache;
use crate::parser::ast::Statement;
use super::build_support::{
    apply_cfg_filter, dedup_llvm_declarations, generate_runtime_declarations,
    generate_struct_constructors, inject_test_harness, precompile_c_object, progress_phase,
    runtime_base,
};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

pub fn compile_file_with_avenys(source_path: &Path, options: &BuildOptions) -> Result<BuildResult> {
    let source = fs::read_to_string(source_path).map_err(|err| {
        crate::error::MireError::runtime(format!(
            "Could not read '{}': {}",
            source_path.display(),
            err
        ))
    })?;
    let source_filename = source_path.display().to_string();
    match compile_file_inner(source_path, options, &source, &source_filename) {
        Ok(result) => Ok(result),
        Err(err) => Err(err.ensure_context(&source_filename, &source)),
    }
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
    let binary_path = options
        .output
        .clone()
        .unwrap_or_else(|| output_dir.join(stem));
    let ir_path = options
        .persist_ir
        .then(|| output_dir.join(format!("{stem}.ll")));
    let optimized_ir_path = options
        .persist_ir
        .then(|| output_dir.join(format!("{stem}.opt.ll")));
    let runtime_base = runtime_base();
    let (c_source_files, c_sources_hash) = if options.emit_binary {
        let mut files = Vec::new();
        for directory in ["runtime", "pal/core", "pal/linux"] {
            super::toolchain::collect_c_files(&runtime_base.join(directory), &mut files)
                .map_err(|err| {
                    MireError::new(ErrorKind::Runtime {
                        span: crate::error::Span::unknown(),
                        message: format!("Could not collect C sources from {directory}: {err}"),
                    })
                })?;
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
    let loaded = load_program_with_cache(source_path, &mut cache, options.import_mode)?;
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
        &format!("{:x}", c_sources_hash),
    );

    if let Some(entry) = cache.build_entry(
        source_path,
        options.mode,
        options.import_mode,
        options.emit_binary,
        options.persist_ir,
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
    let program = if let Some(cached) = cache.cached_analysis(source_path, source_file_hash, dep_fingerprint) {
        match cached {
            CachedAnalysis::Success(mut program) => {
                apply_cfg_filter(&mut program);
                if options.test_mode {
                    inject_test_harness(&mut program);
                }
                program
            }
            CachedAnalysis::Error(error) => return Err(error),
        }
    } else {
        let mut program = loaded.program;
        apply_cfg_filter(&mut program);
        if options.test_mode {
            inject_test_harness(&mut program);
        }
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
            cache.store_analysis_error(source_path, source_file_hash, dep_fingerprint, &program, &err)?;
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
    if let Some(err_diag) = warnings.iter().find(|d| matches!(d.severity, Severity::Error)) {
        return Err(MireError::from_diagnostic(err_diag));
    }

    let (mut ir, extern_libs) = {
        let mut mir = lower_program_with_filename(&program, source_filename);

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
        // Add @main entry point wrapper if the program defines @fn_main
        if ir.contains("define") && ir.contains("@fn_main") && !ir.contains("define i32 @main(") {
            ir.push_str("\n\ndefine i32 @main(i32 %argc, ptr %argv) {\n");
            ir.push_str("  store i32 %argc, ptr @.argc\n");
            ir.push_str("  store ptr %argv, ptr @.argv\n");
            ir.push_str("  %call_main = call i64 @fn_main(ptr null)\n");
            ir.push_str("  ret i32 0\n");
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
        let cache_dir = runtime_base.join(".cobject_cache");
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
        compile_binary_from_ir(
            &final_ir,
            &c_objects,
            &binary_path,
            &extern_libs,
            options.opt_level,
            source_filename,
        )?;
        let phase_link = build_start.elapsed().as_millis() as u64;
        progress_phase(
            "link",
            source_filename,
            phase_link - phase_llvm,
            phase_link,
        );
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
        .join(match mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        })
}
