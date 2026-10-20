use super::*;
use crate::avens::config::{CDefs, OptLevel};
use crate::parser::ast::DataType;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

static C_PRECOMPILE_SEQ: AtomicU64 = AtomicU64::new(0);

static C_EXTRA: OnceLock<CDefs> = OnceLock::new();

pub(super) fn set_c_defs(d: CDefs) {
    let _ = C_EXTRA.set(d);
}

pub(crate) fn c_defs() -> &'static CDefs {
    C_EXTRA.get_or_init(CDefs::default)
}

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

/// Collect C sources from a runtime subtree, including its organized modules.
pub(super) fn collect_c_sources(directory: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    fn visit(directory: &std::path::Path, sources: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(&path, sources)?;
            } else if path.extension().is_some_and(|extension| extension == "c") {
                sources.push(path);
            }
        }
        Ok(())
    }

    let mut sources = Vec::new();
    visit(directory, &mut sources)?;
    sources.sort();
    Ok(sources)
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

pub(super) fn struct_field_align(dt: &DataType) -> usize {
    match dt {
        DataType::I128 | DataType::U128 => 16,
        DataType::I64 | DataType::Char | DataType::U64 | DataType::F64 | DataType::None => 8,
        DataType::I32 | DataType::U32 | DataType::F32 => 4,
        DataType::I16 | DataType::U16 => 2,
        DataType::I8 | DataType::U8 | DataType::Bool => 1,
        DataType::Array { element_type, .. } => struct_field_align(element_type),
        _ => 8,
    }
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
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
            "declare ptr @rt_math_range_between_i64(",
            "declare ptr @rt_math_range_between_i64(i64, i64)",
        ),
        (
            "declare ptr @rt_math_range_step_i64(",
            "declare ptr @rt_math_range_step_i64(i64, i64, i64)",
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
        ("declare ptr @ireru(", "declare ptr @ireru(ptr)"),
        ("declare i32 @fflush(", "declare i32 @fflush(ptr)"),
        ("declare i32 @strcmp(", "declare i32 @strcmp(ptr, ptr)"),
        (
            "declare void @rt_managed_free(",
            "declare void @rt_managed_free(ptr)",
        ),
        (
            "call void @rt_managed_retain(",
            "declare void @rt_managed_retain(ptr)",
        ),
        (
            "declare ptr @rt_string_concat(",
            "declare ptr @rt_string_concat(ptr, ptr)",
        ),
        (
            "declare ptr @rt_string_concat_n(",
            "declare ptr @rt_string_concat_n(i64, ptr)",
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

            let param_types: Vec<String> = fields
                .iter()
                .filter_map(|f| {
                    if let Statement::Let { data_type, .. } = f {
                        Some(match data_type {
                            // Arrays are passed by value ([N x T]) to match the
                            // caller-side materialization in lower_call_args.
                            DataType::Array { .. } => struct_field_llvm_body_type(data_type),
                            _ => struct_field_llvm_type(data_type).to_string(),
                        })
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

            // Match LLVM's struct layout: each field is aligned to its ABI
            // alignment and the whole struct is padded to its maximum
            // alignment. Summing field sizes under-allocates mixed-width
            // structs (e.g. { i8, i64 } is 16 bytes, not 9).
            let mut total_size = 0usize;
            let mut struct_align = 1usize;
            for field in fields {
                if let Statement::Let { data_type, .. } = field {
                    let field_align = struct_field_align(data_type);
                    total_size = align_up(total_size, field_align);
                    total_size += struct_field_size(data_type);
                    struct_align = struct_align.max(field_align);
                }
            }
            total_size = align_up(total_size, struct_align);

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
                            body.push_str(&format!("  store {bty} %{i}, ptr %f{i}_ptr\n"));
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

/// Generates heap constructor functions for every enum variant. Each variant is a
/// tagged struct: an `i64` discriminant followed by the variant's payload fields.
/// The layout must match `render_struct_llvm_type` for the same variant in the MIR
/// `struct_types` map (discriminant is always field 0).
pub(super) fn generate_enum_constructors(program: &crate::parser::ast::Program) -> String {
    let mut out = String::new();
    for stmt in &program.statements {
        if let Statement::Enum { name, variants, .. } = stmt {
            for (discriminant, variant) in variants.iter().enumerate() {
                let full_name = format!("{}.{}", name, variant.name);
                let mut all_types = vec!["i64".to_string()];
                let mut total_size = 8usize;
                let mut enum_align = 8usize;
                for dt in &variant.data_types {
                    all_types.push(struct_field_llvm_body_type(dt));
                    let field_align = struct_field_align(dt);
                    total_size = align_up(total_size, field_align);
                    total_size += struct_field_size(dt);
                    enum_align = enum_align.max(field_align);
                }
                total_size = align_up(total_size, enum_align);
                let struct_ty = all_types.join(", ");
                let param_types: Vec<String> = variant
                    .data_types
                    .iter()
                    .map(|dt| match dt {
                        DataType::Array { .. } => struct_field_llvm_body_type(dt),
                        _ => struct_field_llvm_type(dt).to_string(),
                    })
                    .collect();
                let params: Vec<String> = param_types
                    .iter()
                    .enumerate()
                    .map(|(i, ft)| format!("{} %{}", ft, i))
                    .collect();

                let mut body = String::new();
                body.push_str(&format!("  %ptr = call ptr @malloc(i64 {total_size})\n"));
                body.push_str(&format!(
                    "  %d0 = getelementptr inbounds {{ {struct_ty} }}, ptr %ptr, i32 0, i32 0\n"
                ));
                body.push_str(&format!("  store i64 {discriminant}, ptr %d0\n"));
                for (i, dt) in variant.data_types.iter().enumerate() {
                    let bty = &param_types[i];
                    body.push_str(&format!(
                        "  %f{i}_ptr = getelementptr inbounds {{ {struct_ty} }}, ptr %ptr, i32 0, i32 {}\n",
                        i + 1
                    ));
                    match dt {
                        DataType::Array { .. } => {
                            body.push_str(&format!("  store {bty} %{i}, ptr %f{i}_ptr\n"));
                        }
                        _ => {
                            body.push_str(&format!(
                                "  store {} %{i}, ptr %f{i}_ptr\n",
                                struct_field_llvm_type(dt),
                            ));
                        }
                    }
                }
                body.push_str("  ret ptr %ptr\n");

                out.push_str(&format!(
                    "define ptr @{}({}) {{\nentry:\n{}}}\n\n",
                    full_name,
                    params.join(", "),
                    body,
                ));
            }
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

pub(super) fn c_object_hash(
    content: &str,
    position_independent: bool,
    cflags: &[String],
    opt_level: OptLevel,
    tcc_path: &str,
) -> u64 {
    let mut hasher = crate::incremental::FxHasher::new();
    content.hash(&mut hasher);
    // The same C source must not share an object between executable and shared
    // artifact builds: shared objects require PIC while normal objects do not.
    // Include flags as well so a changed ABI/compiler option cannot reuse an
    // incompatible cached object.
    position_independent.hash(&mut hasher);
    cflags.hash(&mut hasher);
    opt_level.hash(&mut hasher);
    // MIRE_HAVE_TLS changes how tls_compat.h spells every thread-local
    // declaration, so an object built by a TLS-capable compiler must not be
    // reused for a compiler without TLS (and the other way round).
    tcc_supports_tls(tcc_path).hash(&mut hasher);
    hasher.finish()
}

/// Localiza el compilador Tiny C (tcc / libtcc).
/// Orden de preferencia:
///   1. variable de entorno $TCC_PATH (si apunta a un ejecutable existente)
///   2. `tcc` en PATH
///   3. rutas habituales de instalación
///
/// $TCC_PATH va primero porque es la única forma de señalar "usa este tcc
/// concreto". Con el orden anterior, un `tcc` de distribución en el PATH
/// ganaba siempre y el valor exportado se ignoraba en silencio.
pub(super) fn find_tcc() -> Option<String> {
    if let Ok(env) = std::env::var("TCC_PATH")
        && !env.is_empty()
        && std::path::Path::new(&env).exists()
    {
        return Some(env);
    }
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in path_env.split(':') {
            let p = std::path::Path::new(dir).join("tcc");
            if p.exists() {
                return Some("tcc".to_string());
            }
        }
    }
    for p in ["/usr/bin/tcc", "/usr/local/bin/tcc", "/opt/bin/tcc"] {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    None
}

/// Cachea el resultado del sondeo de TLS por ruta de compilador.
static TCC_TLS_CAPABLE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, bool>>,
> = std::sync::OnceLock::new();

/// Sondea si el tcc elegido soporta almacenamiento thread-local.
///
/// Ambas compilaciones de tcc de esta máquina (la de Arch y la de git)
/// declaran `__TINYC__ 928` y `__STDC_VERSION__ 199901L`, así que ninguna
/// prueba de preprocesador las distingue. La única señal fiable es compilar
/// algo: el tcc de Arch rechaza `__thread` con "';' expected (got 'int')" y
/// `_Thread_local` con "_Thread_local is not implemented".
///
/// El sondeo ocurre una vez por ruta de compilador y por proceso. Si el
/// compilador no se puede ejecutar, se asume que sí soporta TLS: ese es el
/// caso de todos los compiladores salvo un tcc antiguo, y así un fallo
/// transitorio de la sonda no degrada silenciosamente el runtime.
fn tcc_supports_tls(tcc_path: &str) -> bool {
    let cache =
        TCC_TLS_CAPABLE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Ok(map) = cache.lock()
        && let Some(&known) = map.get(tcc_path)
    {
        return known;
    }
    let verdict = probe_tcc_tls(tcc_path);
    if let Ok(mut map) = cache.lock() {
        map.insert(tcc_path.to_string(), verdict);
    }
    verdict
}

fn probe_tcc_tls(tcc_path: &str) -> bool {
    // One directory per call, not per process: precompilation runs on several
    // threads and a shared directory meant one thread's cleanup deleted the
    // directory another was still writing into ("could not write probe.o: No
    // such file or directory").
    static PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = (std::process::id() as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(PROBE_SEQ.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("mire-tls-probe-{unique}"));
    if fs::create_dir_all(&dir).is_err() {
        return true;
    }
    let src = dir.join("probe.c");
    // Both spellings appear so a compiler that only has one of them still
    // counts as capable.
    let body = "__thread int a;\n_Thread_local int b;\nint main(void){return a+b;}\n";
    if fs::write(&src, body).is_err() {
        let _ = fs::remove_dir(&dir);
        return true;
    }
    let out = dir.join("probe.o");
    let status = std::process::Command::new(tcc_path)
        .args(["-c", "-o"])
        .arg(&out)
        .arg(&src)
        // The probe is expected to fail on a compiler without TLS. Its
        // diagnostics must not reach the user as if they came from the build.
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let ok = matches!(status, Ok(s) if s.success());
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&out);
    let _ = fs::remove_dir(&dir);
    ok
}

pub(super) fn precompile_c_object(
    c_path: &str,
    cache_dir: &Path,
    runtime_base: &Path,
    opt_level: OptLevel,
) -> Result<String> {
    let content = fs::read_to_string(c_path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::new(1, 1),
            message: format!("Could not read C source '{}': {}", c_path, err),
        })
    })?;
    let extra = c_defs();
    let is_shared = matches!(extra.artifact, super::config::LibType::Shared);
    // Resolve the compiler before hashing: the object key depends on whether
    // that compiler can do thread-local storage.
    let tcc_path = find_tcc().ok_or_else(|| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: "libtcc (tcc) not found. Install tcc or set the TCC_PATH environment variable. C object compilation (runtime, PAL) requires libtcc."
                .to_string(),
        })
    })?;
    let hash = c_object_hash(&content, is_shared, &extra.cflags, opt_level, &tcc_path);
    fs::create_dir_all(cache_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not create cobjects dir: {}", err),
        })
    })?;
    let obj_path = cache_dir.join(format!("{:x}.o", hash));
    // Fast path: a concurrent compiler or an earlier build already published a
    // complete (byte-identical, content-keyed) object. Readers only ever see
    // a fully-renamed file, so this is always a valid read.
    if obj_path.exists() {
        return Ok(obj_path.to_string_lossy().to_string());
    }
    // Compile into a UNIQUE temp object so that concurrent compilers for the
    // SAME cache key (identical source hash -> identical object) never
    // truncate one another's in-progress output. We then publish ours
    // atomically via rename; on Linux `rename` overwrites an existing file
    // with the new (identical) bytes in one atomic step.
    let unique = (std::process::id() as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(C_PRECOMPILE_SEQ.fetch_add(1, Ordering::Relaxed));
    let tmp_path = cache_dir.join(format!("{:x}.{:016x}.o", hash, unique));
    let mut cmd = std::process::Command::new(&tcc_path);
    cmd.args(["-c", opt_level.as_opt_flag(), "-o"])
        .arg(&tmp_path)
        .arg(c_path);
    if is_shared {
        cmd.arg("-fPIC");
    }
    cmd.arg("-I").arg(runtime_base.join("runtime"));
    cmd.arg("-I").arg(runtime_base.join("pal"));
    // tls_compat.h keys off this. See probe_tcc_tls for why it cannot be
    // detected inside the preprocessor.
    if tcc_supports_tls(&tcc_path) {
        cmd.arg("-DMIRE_HAVE_TLS=1");
    } else {
        cmd.arg("-DMIRE_HAVE_TLS=0");
    }
    for inc in &extra.include {
        cmd.arg("-I").arg(inc);
    }
    for flag in &extra.cflags {
        cmd.arg(flag);
    }
    let status = cmd.status().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Failed to run tcc for '{}': {}", c_path, err),
        })
    })?;
    if !status.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("tcc -c failed for '{}'", c_path),
        }));
    }
    // Atomic publish. A sibling compiler may have landed an identical .o in
    // the meantime; rename still atomically replaces it. If rename loses a
    // race (no EEXIST on Linux rename-overwrite), we simply drop our temp —
    // the winner's bytes are identical.
    if let Err(err) = fs::rename(&tmp_path, &obj_path) {
        // rename only fails on real errors (cross-device, ENOENT of temp,
        // permissions); a pre-existing obj_path does NOT cause failure on
        // Linux. Treat a missing temp as "sibling won" only if obj exists.
        if obj_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        } else {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Could not publish C object '{}': {}", c_path, err),
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

/// Replace the body of every `@[test, ignore]` function with a trivial one.
///
/// `ignore` is how a test is quarantined without deleting it, and the harness
/// already skips calling it. Its *body*, though, still reached the type checker
/// and the backend, because analysis runs before the harness is injected. A test
/// quarantined for a compile failure therefore took its whole file down with it:
/// `tests/advanced.mire` holds one ignored closure test and lost all thirteen of
/// its cases, reported as a single type error in a function nobody runs. An
/// ignored test has to stop being compiled as well as stop being run.
///
/// The signature is kept so the function still type-checks and still satisfies
/// anything that references it; only the body is replaced, by a `return` of a
/// default for the declared return type. A `bool` test returns `true`, which is
/// also what a quarantined test would report if the harness ever did call it.
pub(super) fn neutralise_ignored_tests(program: &mut crate::parser::ast::Program) {
    use crate::parser::ast::{Expression, Statement};

    for stmt in &mut program.statements {
        let Statement::Function {
            attributes,
            body,
            return_type,
            ..
        } = stmt
        else {
            continue;
        };
        let is_test = attributes.iter().any(|a| a.name == "test");
        let is_ignored = attributes.iter().any(|a| a.name == "ignore");
        if !is_test || !is_ignored {
            continue;
        }
        body.clear();
        if let Some(value) = default_literal_for(return_type) {
            body.push(Statement::Return(Some(Expression::Literal {
                lit: value,
                line: 0,
                column: 0,
            })));
        }
    }
}

/// A literal of the right kind for `ty`, used to give a bodyless function a
/// valid return. `None` means the type has no sensible literal, in which case the
/// body stays empty and the function falls off its end as it would anyway.
fn default_literal_for(ty: &crate::parser::ast::DataType) -> Option<crate::parser::ast::Literal> {
    use crate::parser::ast::{DataType, Literal};
    Some(match ty {
        DataType::Bool => Literal::Bool(true),
        DataType::I8
        | DataType::I16
        | DataType::I32
        | DataType::I64
        | DataType::I128
        | DataType::U8
        | DataType::U16
        | DataType::U32
        | DataType::U64
        | DataType::U128
        | DataType::Char => Literal::Int(0),
        DataType::F32 | DataType::F64 => Literal::Float(0.0),
        DataType::Str => Literal::Str(String::new()),
        _ => return None,
    })
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
                    args: vec![Expression::Literal {
                        lit: Literal::Str(format!("\n  [{}]", current_section)),
                        line: 0,
                        column: 0,
                    }],
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
                args: vec![Expression::Literal {
                    lit: Literal::Str(format!("  [SKIP] {}", test.name)),
                    line: 0,
                    column: 0,
                }],
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
                    args: vec![Expression::Literal {
                        lit: Literal::Str(format!("  [PASS] {}", test.name)),
                        line: 0,
                        column: 0,
                    }],
                    type_args: Vec::new(),
                    name_line: 0,
                    name_column: 0,
                    data_type: DataType::None,
                })],
                else_branch: Some(vec![Statement::Expression(Expression::Call {
                    name: "dasu".to_string(),
                    args: vec![Expression::Literal {
                        lit: Literal::Str(format!("  [FAIL] {}", test.name)),
                        line: 0,
                        column: 0,
                    }],
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

/// Auto-scope project + dependency `[macros]` declarations so that macro
/// functions declared with `@[macro!]` in a library are usable as `name!(...)`
/// in any project that depends on it — without a `load` at the call site.
///
/// Only the *project's own* `[macros]` and the `[macros]` of its direct
/// dependencies are scanned (macro libraries don't chain further). Injected
/// statements are de-duplicated by kind/name so re-running on the cached path
/// is safe (this is only called on the uncached build path).
///
/// In strict security mode, macros from dependencies are only injected when
/// the dependency's trust tier is `macros` or `ffi`.
pub(super) fn inject_macros(
    program: &mut crate::parser::ast::Program,
    source_path: &std::path::Path,
) {
    use crate::loader::resolve_dependency_root;
    use crate::parser::ast::Statement;
    use crate::parser::parse_with_recovery;
    use std::collections::HashSet;
    use std::path::Path;

    let project_root = match find_project_root(source_path) {
        Some(root) => root,
        None => return,
    };

    let security_config = load_security_config();

    // Names already present in the program (macros may be declared locally).
    let mut existing_fns: HashSet<String> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let mut existing_externs: HashSet<String> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::ExternFunction { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let existing_loads: HashSet<Vec<String>> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            Statement::Load { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect();

    let mut inject_from = |macros: &MireMacros, root: &Path, _source_label: &str| {
        for rel_path in macros.entries.values() {
            let Some(file) = resolve_macro_file(root, rel_path) else {
                continue;
            };
            let src = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (parsed, _) = parse_with_recovery(&src);
            for stmt in parsed.statements {
                match &stmt {
                    Statement::ExternFunction { name: ename, .. } => {
                        if existing_externs.insert(ename.clone()) {
                            program.statements.push(stmt);
                        }
                    }
                    Statement::Load { path, .. } => {
                        if !existing_loads.contains(path) {
                            program.statements.push(stmt);
                        }
                    }
                    Statement::Function {
                        name: fname,
                        attributes,
                        ..
                    } if attributes.iter().any(|a| a.name == "macro!")
                        && existing_fns.insert(fname.clone()) =>
                    {
                        program.statements.push(stmt);
                    }
                    _ => {}
                }
            }
        }
    };

    if let Ok(Some(manifest)) = load_project_manifest(&project_root)
        && let Some(macros) = &manifest.macros
    {
        inject_from(macros, &project_root, "this project's owl.toml");
    }

    if let Ok(Some(manifest)) = load_project_manifest(&project_root) {
        let strict = security_config
            .as_ref()
            .map(|c| c.mode == SecurityMode::Strict)
            .unwrap_or(false);
        for (dep_name, dep) in manifest.dependencies.entries.iter() {
            if strict {
                let tier = security_config
                    .as_ref()
                    .and_then(|c| c.deps.get(dep_name).copied())
                    .unwrap_or(TrustTier::Code);
                if tier < TrustTier::Macros {
                    continue;
                }
            }
            let Some(root) = resolve_dependency_root(&project_root, dep_name, dep) else {
                continue;
            };
            if let Ok(Some(dep_manifest)) = load_project_manifest(&root)
                && let Some(dep_macros) = &dep_manifest.macros
            {
                inject_from(dep_macros, &root, dep_name);
            }
        }
    }
}

fn load_security_config() -> Option<SecurityConfig> {
    let cwd = std::env::current_dir().ok()?;
    let manifest = crate::avens::load_project_manifest(&cwd).ok()??;
    manifest.security
}

fn resolve_macro_file(root: &std::path::Path, rel_path: &str) -> Option<std::path::PathBuf> {
    let check = |p: &std::path::Path| -> Option<std::path::PathBuf> {
        let c = p.canonicalize().ok()?;
        // Macro paths are explicitly declared in owl.toml. They may point at
        // a dependency (for example ../mire/core/macros/assert.mire), so the
        // project-root confinement that silently dropped them is not applied.
        if c.is_file() { Some(c) } else { None }
    };
    let candidate = root.join(rel_path);
    if let Some(file) = check(&candidate) {
        return Some(file);
    }
    let with_ext = candidate.with_extension("mire");
    if let Some(file) = check(&with_ext) {
        return Some(file);
    }
    let with_mr_ext = candidate.with_extension("mr");
    if let Some(file) = check(&with_mr_ext) {
        return Some(file);
    }
    let mod_file = candidate.join("mod.mire");
    if let Some(file) = check(&mod_file) {
        return Some(file);
    }
    check(&candidate.join("mod.mr"))
}

// ── Dependency Collector ──────────────────────────────────────────────────────
// Unified dependency collector: scans the generated LLVM IR and returns the set
// of PAL and runtime symbols that are actually called by the program.
//
// This replaces the two previous mechanisms:
//   pal_extern_decls()        → emitted ALL ~150 PAL/runtime declarations
//   generate_runtime_declarations() → emitted only used rt_* declarations
//
// The collector is a single-pass scan over the IR text looking for `call @pal_*`
// and `call @rt_*` patterns.  The results are used to:
//   1. Filter pal_extern_decls() to emit only used PAL declarations
//   2. Determine which C source files to compile (tier-aware)
//   3. Enforce the tier contract (none = no rt_* calls allowed)
//
// Runtime and PAL are separate concerns:
//   Runtime = language features (rt_div_i64, rt_string_concat, rt_bounds_fail, ...)
//   PAL     = host interaction (pal_file_open, pal_proc_create, pal_channel_send, ...)
// A program can use PAL without the runtime, or the runtime without PAL.

use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct UsedSymbols {
    pub runtime: HashSet<String>,
    pub pal: HashSet<String>,
}

/// Scan the IR for `call @sym_name(...)` patterns and collect all used symbols.
/// Returns a set of (runtime, pal) symbol names that are actually invoked.
pub(crate) fn collect_used_symbols(ir: &str) -> UsedSymbols {
    let mut used = UsedSymbols::default();
    for line in ir.lines() {
        let trimmed = line.trim();
        // Match call instructions: `... call ... @sym_name(`
        // This handles both tail-call form (`call ... @sym(`) and
        // assignment form (`%r = call ptr @sym(...)`).
        if let Some(call_pos) = trimmed.find(" call ") {
            let rest = &trimmed[call_pos + 6..]; // skip " call "
            if let Some(at_pos) = rest.find('@') {
                let after_at = &rest[at_pos + 1..];
                if let Some(paren_pos) = after_at.find('(') {
                    let sym = &after_at[..paren_pos];
                    if sym.starts_with("pal_") {
                        used.pal.insert(sym.to_string());
                    } else if sym.starts_with("rt_") || sym == "ireru" {
                        used.runtime.insert(sym.to_string());
                    }
                }
            }
        }
    }
    used
}

/// Filter `pal_extern_decls()` to emit only declarations for symbols that appear
/// in the `used_pal` set.
///
/// The declaration set is filtered by actual use rather than by tier. Emitting
/// every declaration unconditionally was the previous `full`-tier behavior; it
/// linked symbols no program referenced, and measurement showed no benefit over
/// emitting only what is used.
pub(crate) fn filter_pal_decls(used_pal: &HashSet<String>) -> Vec<String> {
    let all = crate::compiler::mir::codegen::builtins::pal_extern_decls();
    all.into_iter()
        .filter(|decl| {
            // Extract the symbol name from `declare ... @sym_name(...)`
            if let Some(at_pos) = decl.find('@') {
                let rest = &decl[at_pos + 1..];
                if let Some(paren_pos) = rest.find('(') {
                    let sym = &rest[..paren_pos];
                    // Include the symbol if it's used, or if it's a runtime helper
                    // (runtime helpers are always needed when the program uses rt_* functions)
                    return used_pal.contains(sym) || sym.starts_with("rt_");
                }
            }
            false
        })
        .collect()
}

/// Strip unused PAL and runtime declarations from the IR.
/// This is used post-generation to remove dead declarations that were emitted
/// before the dependency collector ran (e.g., from the old `pal_extern_decls()` call).
#[allow(dead_code)]
pub(super) fn strip_unused_decls(ir: &str, used: &UsedSymbols) -> String {
    ir.lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Keep non-declaration lines
            if !trimmed.starts_with("declare") {
                return true;
            }
            // Extract the symbol name from `declare ... @sym_name(...)`
            if let Some(at_pos) = trimmed.find('@') {
                let rest = &trimmed[at_pos + 1..];
                if let Some(paren_pos) = rest.find('(') {
                    let sym = &rest[..paren_pos];
                    // Keep the declaration if the symbol is used
                    return used.runtime.contains(sym) || used.pal.contains(sym);
                }
            }
            // Keep unknown declarations (safety net)
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Map a runtime symbol name to the C source file that provides it.
/// Returns the path relative to `runtime/` or `None` if unknown.
fn runtime_symbol_to_c_file(sym: &str) -> Option<&'static str> {
    // Order matters: more specific prefixes first.
    //
    if sym.starts_with("rt_time_elapsed_ms") {
        Some("core/time.c")
    } else if sym.starts_with("rt_math_random") {
        Some("core/random.c")
    } else if sym.starts_with("rt_math_") {
        Some("core/math.c")
    } else if sym.starts_with("rt_maybe_")
        || sym.starts_with("rt_result_")
        || sym.starts_with("rt_arr_")
    {
        Some("core/mire_types.c")
    } else if sym.starts_with("rt_managed_") || sym.starts_with("rt_panic") {
        Some("memory/allocator/managed.c")
    } else if sym.starts_with("rt_check_bounds") {
        Some("memory/safety/safety.c")
    } else if sym == "ireru"
        || sym.starts_with("rt_io_read")
        || sym == "rt_io_available"
        || sym == "rt_io_is_tty"
    {
        Some("std/input/input.c")
    } else if sym.starts_with("rt_io_write") || sym == "rt_io_flush" {
        Some("std/output/output.c")
    } else if sym == "rt_io_error" || sym == "rt_io_clear" || sym == "rt_io_fd" {
        Some("std/error/error.c")
    } else if sym.starts_with("rt_list_")
        || sym.starts_with("rt_lists_")
        || sym.starts_with("rt_vecs_")
    {
        Some("collections/vec/vecs.c")
    } else if sym.starts_with("rt_dict_")
        || sym.starts_with("rt_dicts_")
        || sym.starts_with("rt_maps_")
    {
        Some("collections/map/maps.c")
    } else if sym.starts_with("rt_string_")
        || sym.starts_with("rt_strings_")
        || sym.starts_with("rt_i64_to_string")
        || sym.starts_with("rt_i128_to_string")
        || sym.starts_with("rt_u128_to_string")
        || sym.starts_with("rt_f64_to_string")
        || sym.starts_with("rt_f32_to_string")
        || sym.starts_with("rt_bool_to_string")
        || sym.starts_with("rt_unicode_")
        || sym.starts_with("rt_managed_from")
        || sym.starts_with("rt_managed_ensure")
    {
        Some("core/strings.c")
    } else if sym.starts_with("rt_closure_env") {
        Some("memory/safety/safety.c")
    } else if sym.starts_with("rt_read_bytes") || sym.starts_with("rt_read_tty") {
        // File/tty readers (need POSIX; stay in helpers.c)
        Some("pal_bridge/helpers.c")
    } else if sym.starts_with("rt_crypto_")
        || sym.starts_with("rt_hex_to_file")
        || sym.starts_with("rt_proc_")
        || sym.starts_with("rt_channel_recv")
        || sym.starts_with("rt_fs_")
        || sym.starts_with("rt_font_")
        || sym.starts_with("rt_web_")
        || sym.starts_with("rt_websocket_")
    {
        Some("pal_bridge/helpers.c")
    } else if sym.starts_with("rt_alloc_raw")
        || sym.starts_with("rt_free_raw")
        || sym.starts_with("rt_ptr_")
        || sym.starts_with("rt_blend_")
        || sym.starts_with("rt_read_")
        || sym.starts_with("rt_write_")
    {
        // Pure byte-access helpers (PAL-free; math.c, FFI bindings)
        Some("core/bytes.c")
    } else if sym.starts_with("rt_thread_") {
        Some("pal_bridge/thread.c")
    } else if sym.starts_with("rt_get_args") || sym.starts_with("rt_free_argv") {
        Some("pal_bridge/helpers.c")
    } else if sym.starts_with("rt_pal_") {
        // Scalar FFI adapters for PAL handles. Mire represents handles as i64;
        // the PAL keeps typed {index, generation} values internally.
        Some("pal_bridge/pal_bridge.c")
    } else if sym.starts_with("rt_sbx_") {
        // Sandbox bridges. They live in helpers.c rather than beside the policy in
        // pal_sandbox.c so that a program declaring one still resolves them on the
        // minimal runtime tier: that tier only extends the C source list with
        // pal/core when the IR mentions a pal_* symbol, and an rt_sbx_ declaration
        // on its own would then leave these two undefined. helpers.c is already a
        // dependency of most runtime builds and bridges into the PAL at the C
        // level, so the symbol resolves on either tier.
        Some("pal_bridge/helpers.c")
    } else {
        None
    }
}

/// Given a set of used runtime symbols, return the minimal set of C source files
/// needed to satisfy them.  Always includes `managed.c` (memory management) and
/// `safety.c` (panic) as safety baselines.  Transitive C→C dependencies are
/// resolved (e.g. `strings.c` needs `vecs.c` which needs `strings.c`).
pub(crate) fn minimal_runtime_c_files(used_runtime: &HashSet<String>) -> Vec<String> {
    // Map a symbol to its C file (order: more specific prefixes first).
    fn sym_to_file(sym: &str) -> Option<&'static str> {
        runtime_symbol_to_c_file(sym)
    }

    // C-to-C dependency graph (directed, with a strings/vecs cycle).
    // A file listed here needs all the files it points to.
    fn c_deps(file: &str) -> &'static [&'static str] {
        match file {
            "core/strings.c" => &["collections/vec/vecs.c", "memory/allocator/managed.c"],
            "collections/vec/vecs.c" => &[
                "core/strings.c",
                "memory/allocator/managed.c",
                "memory/safety/safety.c",
            ],
            // maps.c delegates hashing, storage sizing and growth to the
            // internal implementation; keeping this edge explicit is
            // required for minimal runtime builds and shared-library tests.
            "collections/map/maps.c" => &[
                "collections/map/maps_internal.c",
                "collections/vec/vecs.c",
                "memory/allocator/managed.c",
            ],
            "collections/map/maps_internal.c" => &["memory/allocator/managed.c"],
            "core/math.c" => &["collections/vec/vecs.c", "core/bytes.c"],
            "core/random.c" => &[],
            "core/time.c" => &["core/strings.c", "memory/allocator/managed.c"],
            "core/mire_types.c" => &[
                "core/strings.c",
                "memory/allocator/managed.c",
                "memory/safety/safety.c",
            ],
            "core/bytes.c" => &["core/strings.c"],
            "pal_bridge/helpers.c" => &["core/strings.c", "memory/allocator/managed.c"],
            "std/input/input.c" => &["memory/allocator/managed.c"],
            "std/output/output.c" => &["memory/allocator/managed.c"],
            "std/error/error.c" => &[],
            "pal_bridge/thread.c" => &[],
            "memory/safety/safety.c" => &["std/error/error.c"],
            "memory/allocator/managed.c" => &["std/error/error.c"],
            _ => &[],
        }
    }

    let mut needed: HashSet<String> = HashSet::new();
    // Always include managed and safety (core infrastructure)
    needed.insert("memory/allocator/managed.c".to_string());
    needed.insert("memory/safety/safety.c".to_string());
    // Add files directly needed by used symbols
    for sym in used_runtime {
        if let Some(file) = sym_to_file(sym) {
            needed.insert(file.to_string());
        }
    }
    // Resolve transitive C→C dependencies (fixed-point iteration, max 4 passes
    // because the longest chain is managed→strings→vecs→strings, stabilizes fast).
    for _ in 0..4 {
        let current: Vec<String> = needed.iter().cloned().collect();
        for file in &current {
            for &dep in c_deps(file) {
                needed.insert(dep.to_string());
            }
        }
    }
    let mut files: Vec<String> = needed.into_iter().collect();
    files.sort();
    files
}

pub(crate) fn runtime_c_file_needs_pal(source_path: &std::path::Path) -> bool {
    matches!(
        source_path.file_name().and_then(|name| name.to_str()),
        Some(
            "error.c"
                | "helpers.c"
                | "input.c"
                | "managed.c"
                | "managed_minimal.c"
                | "maps.c"
                | "maps_internal.c"
                | "math.c"
                | "mire_types.c"
                | "output.c"
                | "vecs.c"
                | "strings.c"
                | "strings_minimal.c"
                | "thread.c"
                | "pal_bridge.c"
                | "time.c",
        )
    )
}

#[cfg(test)]
mod runtime_source_tests {
    use super::{
        collect_c_sources, collect_used_symbols, generate_runtime_declarations,
        minimal_runtime_c_files, runtime_c_file_needs_pal, runtime_symbol_to_c_file,
    };
    use std::{collections::HashSet, path::Path};

    #[test]
    fn scalar_pal_socket_bridges_select_the_runtime_bridge_and_pal_backend() {
        let symbols = HashSet::from(["rt_pal_unix_connect".to_string()]);
        let runtime_files = minimal_runtime_c_files(&symbols);

        assert_eq!(
            runtime_symbol_to_c_file("rt_pal_unix_connect"),
            Some("pal_bridge/pal_bridge.c")
        );
        assert!(
            runtime_files
                .iter()
                .any(|file| file == "pal_bridge/pal_bridge.c")
        );
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/pal_bridge/pal_bridge.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/std/input/input.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/std/output/output.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/std/error/error.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/collections/vec/vecs.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/collections/map/maps.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/collections/map/maps_internal.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/core/strings.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/core/strings_minimal.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new("runtime/core/math.c")));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/core/mire_types.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/memory/allocator/managed.c"
        )));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/memory/allocator/managed_minimal.c"
        )));
    }

    #[test]
    fn runtime_symbol_providers_use_the_runtime_module_layout() {
        assert_eq!(
            runtime_symbol_to_c_file("rt_i64_to_string"),
            Some("core/strings.c")
        );
        assert_eq!(
            runtime_symbol_to_c_file("rt_managed_free"),
            Some("memory/allocator/managed.c")
        );
        assert_eq!(
            runtime_symbol_to_c_file("rt_dict_free"),
            Some("collections/map/maps.c")
        );
        assert_eq!(
            runtime_symbol_to_c_file("rt_time_elapsed_ms_str"),
            Some("core/time.c")
        );
        assert!(runtime_c_file_needs_pal(Path::new("runtime/core/time.c")));
        assert_eq!(
            runtime_symbol_to_c_file("rt_io_read_line"),
            Some("std/input/input.c")
        );
        assert_eq!(runtime_symbol_to_c_file("ireru"), Some("std/input/input.c"));
        assert_eq!(
            runtime_symbol_to_c_file("rt_io_write_line"),
            Some("std/output/output.c")
        );
        assert_eq!(
            runtime_symbol_to_c_file("rt_io_error"),
            Some("std/error/error.c")
        );

        let symbols = HashSet::from(["rt_dict_free".to_string()]);
        let files = minimal_runtime_c_files(&symbols);
        assert!(files.iter().any(|file| file == "collections/map/maps.c"));
        assert!(
            files
                .iter()
                .any(|file| file == "collections/map/maps_internal.c")
        );
        assert!(
            files
                .iter()
                .any(|file| file == "memory/allocator/managed.c")
        );
    }

    #[test]
    fn runtime_source_collection_descends_into_module_directories() {
        let runtime_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime");
        let sources = collect_c_sources(&runtime_directory).expect("runtime sources are readable");
        let relative_sources: HashSet<String> = sources
            .iter()
            .map(|source| {
                source
                    .strip_prefix(&runtime_directory)
                    .expect("source is inside runtime")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        for expected in [
            "core/strings.c",
            "core/time.c",
            "memory/allocator/managed.c",
            "collections/vec/vecs.c",
            "collections/map/maps.c",
            "std/input/input.c",
            "std/output/output.c",
            "std/error/error.c",
            "pal_bridge/pal_bridge.c",
        ] {
            assert!(relative_sources.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn input_builtin_call_is_retained_and_selects_the_input_runtime_module() {
        let used = collect_used_symbols("%input = call ptr @ireru(ptr null)");
        assert!(used.runtime.contains("ireru"));
        assert!(
            generate_runtime_declarations("%input = call ptr @ireru(ptr null)")
                .contains("declare ptr @ireru(ptr)")
        );
        assert_eq!(runtime_symbol_to_c_file("ireru"), Some("std/input/input.c"));
        assert!(
            minimal_runtime_c_files(&used.runtime)
                .iter()
                .any(|file| file == "std/input/input.c")
        );
    }

    #[test]
    fn minimal_panic_support_selects_the_pal_diagnostic_provider() {
        let used = HashSet::from(["rt_panic_loc".to_string()]);
        let files = minimal_runtime_c_files(&used);

        assert!(files.iter().any(|file| file == "memory/safety/safety.c"));
        assert!(files.iter().any(|file| file == "std/error/error.c"));
        assert!(runtime_c_file_needs_pal(Path::new(
            "runtime/std/error/error.c"
        )));
    }
}
