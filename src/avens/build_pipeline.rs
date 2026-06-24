use super::*;
use crate::compiler::check_warnings_with_origins;
use crate::compiler::mir::{codegen::mir_to_llvm, lower::lower_program, optimize::optimize};
use crate::loader::load_program_with_cache;
use crate::parser::ast::{DataType, Statement};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

fn runtime_base() -> PathBuf {
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("runtime").exists() {
                return parent.to_path_buf();
            }
            if parent.join("../lib/mire/runtime").exists() {
                return parent.join("../lib/mire");
            }
        }
    }
    manifest_dir.join("src")
}

fn struct_field_llvm_type(dt: &DataType) -> &'static str {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64",
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

fn struct_field_llvm_body_type(dt: &DataType) -> String {
    match dt {
        DataType::Array { element_type, size } => {
            format!("[{} x {}]", size, struct_field_llvm_body_type(element_type))
        }
        _ => struct_field_llvm_type(dt).to_string(),
    }
}

fn struct_field_size(dt: &DataType) -> usize {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => 8,
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

fn generate_runtime_declarations(ir: &str) -> String {
    let mut out = String::new();
    let needed: &[(&str, &str)] = &[
        ("declare ptr @dasu(", "declare ptr @dasu(i64)"),
        ("declare i64 @rt_list_len(", "declare i64 @rt_list_len(ptr)"),
        ("declare i64 @rt_strings_len(", "declare i64 @rt_strings_len(ptr)"),
        ("declare i64 @rt_dicts_len(", "declare i64 @rt_dicts_len(ptr)"),
        ("declare ptr @rt_list_create(", "declare ptr @rt_list_create(i64, i64)"),
        ("declare ptr @rt_list_push_i64(", "declare ptr @rt_list_push_i64(ptr, i64)"),
        ("declare ptr @rt_list_push_ptr(", "declare ptr @rt_list_push_ptr(ptr, ptr)"),
        ("declare ptr @rt_dicts_set_i64(", "declare ptr @rt_dicts_set_i64(ptr, ptr, i64)"),
        ("declare ptr @rt_dicts_set(", "declare ptr @rt_dicts_set(ptr, ptr, ptr)"),
        ("declare ptr @rt_dicts_set_with_kind(", "declare ptr @rt_dicts_set_with_kind(ptr, ptr, ptr, i64)"),
        ("declare ptr @rt_dicts_keys(", "declare ptr @rt_dicts_keys(ptr)"),
        ("declare ptr @rt_dicts_values(", "declare ptr @rt_dicts_values(ptr)"),
        ("declare ptr @rt_dict_to_string(", "declare ptr @rt_dict_to_string(ptr)"),
        ("declare void @rt_panic_division_by_zero(", "declare void @rt_panic_division_by_zero()"),
        ("declare void @rt_panic_out_of_bounds(", "declare void @rt_panic_out_of_bounds()"),
        ("declare i64 @rt_div_i64(", "declare i64 @rt_div_i64(i64, i64)"),
        ("declare i64 @rt_rem_i64(", "declare i64 @rt_rem_i64(i64, i64)"),
        ("declare void @rt_check_bounds_i64(", "declare void @rt_check_bounds_i64(i64, i64)"),
        ("declare ptr @rt_closure_env_alloc(", "declare ptr @rt_closure_env_alloc(i64)"),
        ("declare ptr @rt_math_range_i64(", "declare ptr @rt_math_range_i64(i64)"),
        ("@.fmt_str =", "@.fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\""),
        ("@.fmt_i64 =", "@.fmt_i64 = private unnamed_addr constant [5 x i8] c\"%ld\\0A\\00\""),
        ("@.fmt_f64 =", "@.fmt_f64 = private unnamed_addr constant [6 x i8] c\"%.6g\\0A\\00\""),
        ("@.fmt_float =", "@.fmt_float = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\""),
        ("@.fmt_bool_true =", "@.fmt_bool_true = private unnamed_addr constant [5 x i8] c\"true\\00\""),
        ("@.fmt_bool_false =", "@.fmt_bool_false = private unnamed_addr constant [6 x i8] c\"false\\00\""),
        ("@.fmt_i32 =", "@.fmt_i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\""),
        ("declare ptr @rt_i64_to_string(", "declare ptr @rt_i64_to_string(i64)"),
        ("declare ptr @rt_f64_to_string(", "declare ptr @rt_f64_to_string(double)"),
        ("declare ptr @rt_bool_to_string(", "declare ptr @rt_bool_to_string(i64)"),
        ("declare ptr @rt_get_args(", "declare ptr @rt_get_args(i32, ptr)"),
        ("declare i32 @printf(", "declare i32 @printf(ptr, ...)"),
        ("declare i32 @fflush(", "declare i32 @fflush(ptr)"),
        ("declare i32 @strcmp(", "declare i32 @strcmp(ptr, ptr)"),
        ("declare void @rt_managed_free(", "declare void @rt_managed_free(ptr)"),
        ("declare ptr @rt_string_concat(", "declare ptr @rt_string_concat(ptr, ptr)"),
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

fn generate_struct_constructors(program: &crate::parser::ast::Program) -> String {
    let mut out = String::new();
    for stmt in &program.statements {
        if let Statement::Type { name, fields, .. } = stmt {
            let field_count = fields.len();
            if field_count == 0 {
                continue;
            }

            let param_types: Vec<&str> = fields.iter().filter_map(|f| {
                if let Statement::Let { data_type, .. } = f {
                    Some(struct_field_llvm_type(data_type))
                } else {
                    None
                }
            }).collect();
            let body_types: Vec<String> = fields.iter().filter_map(|f| {
                if let Statement::Let { data_type, .. } = f {
                    Some(struct_field_llvm_body_type(data_type))
                } else {
                    None
                }
            }).collect();

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
                            body.push_str(&format!(
                                "  %f{i}_loaded = load {bty}, ptr %{i}\n"
                            ));
                            body.push_str(&format!(
                                "  store {bty} %f{i}_loaded, ptr %f{i}_ptr\n"
                            ));
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

fn dedup_llvm_declarations(ir: &str) -> String {
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

fn c_object_hash(content: &str) -> u64 {
    let mut hasher = crate::incremental::FxHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn precompile_c_object(c_path: &str, cache_dir: &Path, runtime_base: &Path) -> Result<String> {
    let content = fs::read_to_string(c_path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not read C source '{}': {}", c_path, err),
        })
    })?;
    let hash = c_object_hash(&content);
    fs::create_dir_all(cache_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not create cobjects dir: {}", err),
        })
    })?;
    let obj_path = cache_dir.join(format!("{:x}.o", hash));
    if !obj_path.exists() {
        if std::env::var("MIRE_DEBUG_CACHE").is_ok() {
            eprintln!("[MIR] compiling C object: {} -> {}", c_path, obj_path.display());
        }
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
                    message: format!("Failed to run clang for '{}': {}", c_path, err),
                })
            })?;
        if !status.success() {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!("clang -c failed for '{}'", c_path),
            }));
        }
    }
    Ok(obj_path.to_string_lossy().to_string())
}

fn progress_phase(phase: &str, _file: &str, elapsed_ms: u64, total_ms: u64) {
    if std::env::var("OWL_PROGRESS").is_ok() {
        eprintln!(
            "{{\"phase\":\"{}\",\"elapsed_ms\":{},\"total_ms\":{}}}",
            phase, elapsed_ms, total_ms
        );
    }
}

pub fn compile_file_with_avenys(source_path: &Path, options: &BuildOptions) -> Result<BuildResult> {
    let build_start = std::time::Instant::now();
    let source = fs::read_to_string(source_path)?;
    let source_filename = source_path.display().to_string();
    let output_dir = default_output_dir(source_path, options.mode);
    fs::create_dir_all(&output_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
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
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_base = runtime_base();
    let pal_backend = std::env::var("MIRE_PAL").unwrap_or_else(|_| "linux".to_string());
    let (c_source_files, c_sources_hash) = if options.emit_binary {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(runtime_base.join("runtime")).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not read runtime/: {}", err),
            })
        })? {
            let entry = entry.map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Could not read entry: {}", err),
                })
            })?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "c") {
                files.push(path.to_string_lossy().to_string());
            }
        }
        for entry in std::fs::read_dir(runtime_base.join(format!("pal/{pal_backend}")))
            .map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Could not read pal/{pal_backend}: {}", err),
                })
            })?
        {
            let entry = entry.map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Could not read entry: {}", err),
                })
            })?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "c") {
                files.push(path.to_string_lossy().to_string());
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
    let loaded = load_program_with_cache(source_path, &mut cache, options.import_mode)?;
    let phase_load = build_start.elapsed().as_millis() as u64;
    progress_phase("load", &source_filename, phase_load, phase_load);
    let source_file_hash = source_hash(&source);
    if options.debug_dump
        && let Some(report) = cache.analysis_invalidation_report(source_path, source_file_hash, &loaded.program)
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
        });
    }
    cache.record_build_miss();

    let mut phase_analyse_time = phase_load;
    let mut phase_mir_time = phase_load;
    let program = if let Some(cached) = cache.cached_analysis(source_path, source_file_hash) {
        match cached {
            CachedAnalysis::Success(program) => program,
            CachedAnalysis::Error(error) => return Err(error),
        }
    } else {
        let mut program = loaded.program;
        let analysis_result = if let Some(cached) = cache.latest_successful_analysis(source_path, source_file_hash) {
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
                    &source,
                    &loaded.statement_origins,
                    &loaded.sources,
                    &selection,
                )
                .map(|_| ())
            }
        } else {
            analyze_program_with_origins(
                &mut program,
                &source,
                &loaded.statement_origins,
                &loaded.sources,
            )
            .map(|_| ())
        };

        if let Err(err) = analysis_result {
            let err = if err.source().is_none() {
                err.with_source(source.clone())
            } else {
                err
            };
            let err = if err.filename().is_none() {
                err.with_filename(source_filename.clone())
            } else {
                err
            };
            cache.store_analysis_error(source_path, source_file_hash, &program, &err)?;
            cache.save()?;
            return Err(err);
        }
        cache.store_analysis(source_path, source_file_hash, &program)?;
        phase_analyse_time = build_start.elapsed().as_millis() as u64;
        progress_phase("analyse", &source_filename, phase_analyse_time - phase_load, phase_analyse_time);
        program
    };

    let warnings = check_warnings_with_origins(
        &program,
        &source,
        Some(&source_filename),
        options.warning_filter.clone(),
        options.deny_warnings.clone(),
        &loaded.statement_origins,
        source_path,
    );
    for diagnostic in &warnings {
        eprintln!("{}", format_diagnostic(diagnostic, true));
    }
    if warnings
        .iter()
        .any(|diag| matches!(diag.severity, Severity::Error))
    {
        return Err(MireError::runtime(
            "Compilation aborted due to denied warnings".to_string(),
        ));
    }

    let use_legacy = std::env::var("MIRE_LEGACY_CODEGEN").is_ok();
    let (mut ir, extern_libs) = if use_legacy {
        LlvmIrGen::new().compile_program(&program).map_err(|err| {
            let err = if err.source().is_none() {
                err.with_source(source.clone())
            } else {
                err
            };
            if err.filename().is_none() {
                err.with_filename(source_filename.clone())
            } else {
                err
            }
        })?
    } else {
        let mut mir = lower_program(&program);

        // Compute combined hash of all MIR function bodies for caching
        let mir_hash: u64 = {
            let mut hasher = crate::incremental::FxHasher::new();
            for func in &mir.functions {
                hasher.write_u64(func.body_hash);
            }
            Hasher::finish(&hasher)
        };

        // Check MIR program cache
        let cached_program_ir = cache.get_cached_mir_fn(
            source_path,
            "_program",
            mir_hash,
            options.opt_level,
        );

        if let Some(cached_ir) = cached_program_ir {
            if options.debug_dump {
                eprintln!("[MIR] program cache hit ({} functions)", mir.functions.len());
            }
            (cached_ir, Vec::new())
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
            let ir = mir_to_llvm(&mir).0;
            phase_mir_time = build_start.elapsed().as_millis() as u64;
            progress_phase("mir", &source_filename, phase_mir_time - phase_analyse_time, phase_mir_time);
            if let Err(e) = cache.store_cached_mir_fn(
                source_path,
                "_program",
                mir_hash,
                options.opt_level,
                &ir,
            )
                && options.debug_dump {
                    eprintln!("[MIR] cache store error: {}", e);
                }
            (ir, Vec::new())
        }
    };
    // Append runtime declarations and struct constructor functions (MIR codegen path)
    if !use_legacy {
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
        if ir.contains("define") && ir.contains("@fn_main")
            && !ir.contains("define i32 @main(") {
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
                message: format!("Could not write '{}': {}", path.display(), err),
            })
        })?;
    }
    if options.debug_dump {
        eprintln!("[DEBUG IR]\n{}", ir);
    }

    let final_ir = if matches!(options.opt_level, OptLevel::O0) {
        ir
    } else {
        optimize_ir(&ir, options.opt_level)?
    };
    let phase_llvm = build_start.elapsed().as_millis() as u64;
    progress_phase("llvm", &source_filename, phase_llvm - phase_mir_time, phase_llvm);

    if let Some(path) = &optimized_ir_path {
        fs::write(path, &final_ir).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not write '{}': {}", path.display(), err),
            })
        })?;
    }

    if options.emit_binary {
        let cache_dir = manifest_dir.join(".cobject_cache");
        let c_objects: Vec<String> = if c_source_files.len() <= 1 {
            c_source_files.iter().map(|src| precompile_c_object(src, &cache_dir, &runtime_base)).collect::<Result<_>>()?
        } else {
            let results = std::sync::Mutex::new(vec![String::new(); c_source_files.len()]);
            std::thread::scope(|s| {
                for (i, src) in c_source_files.iter().enumerate() {
                    let results = &results;
                    let cache_dir = &cache_dir;
                    let runtime_base = &runtime_base;
                    s.spawn(move || {
                        if let Ok(obj) = precompile_c_object(src, cache_dir, runtime_base) {
                            results.lock().unwrap()[i] = obj;
                        }
                    });
                }
            });
            let results = results.into_inner().unwrap();
            if results.iter().any(|s| s.is_empty()) {
                return Err(MireError::runtime("C object compilation failed".to_string()));
            }
            results
        };
        compile_binary_from_ir(
            &final_ir,
            &c_objects,
            &binary_path,
            &extern_libs,
            &pal_backend,
        )?;
        let phase_link = build_start.elapsed().as_millis() as u64;
        progress_phase("link", &source_filename, phase_link - phase_llvm, phase_link);
    }
    let phase_done = build_start.elapsed().as_millis() as u64;
    progress_phase("done", &source_filename, 0, phase_done);

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
