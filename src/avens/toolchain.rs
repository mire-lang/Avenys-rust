use super::*;

#[allow(dead_code)]
pub(super) fn collect_c_files(
    dir: &std::path::Path,
    files: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_c_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("c") {
            files.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

/// Tags every function definition with a size-preference attribute.
///
/// LLVM no longer has `-Os`/`-Oz` pipelines; the equivalent is O2 plus an
/// attribute on each function, so the flag alone was not enough and both
/// documented options failed on every build.
fn apply_size_attribute(ir: &str, attribute: &str) -> String {
    let mut out = String::with_capacity(ir.len() + 64);
    for line in ir.lines() {
        // Insert just before the body, which is the position LLVM reads
        // function attributes from. An existing attribute group (`#0`) stays
        // where it is; this just adds to the list.
        if line.starts_with("define ")
            && let Some(body) = line.rfind('{')
        {
            out.push_str(&line[..body]);
            out.push(' ');
            out.push_str(attribute);
            out.push(' ');
            out.push_str(&line[body..]);
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub(super) fn optimize_ir(ir: &str, opt_level: OptLevel, source_filename: &str) -> Result<String> {
    let ir = match opt_level.size_attribute() {
        Some(attribute) => apply_size_attribute(ir, attribute),
        None => ir.to_string(),
    };
    let mut command = Command::new("opt");
    command
        .arg("-S")
        .arg(opt_level.as_opt_flag())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = command.spawn().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Failed to run opt: {}", err),
        })
        .with_filename(source_filename.to_string())
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(ir.as_bytes()).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to stream IR into opt: {}", err),
            })
            .with_filename(source_filename.to_string())
        })?;
    }
    if let Ok(path) = std::env::var("AVENYS_DUMP_IR") {
        let _ = std::fs::write(&path, ir.as_bytes());
    }
    let output = child.wait_with_output().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Failed to wait for opt: {}", err),
        })
        .with_filename(source_filename.to_string())
    })?;
    if !output.status.success() {
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!(
                "opt failed for `{}` with status {}.\nstderr:\n{}",
                source_filename,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        })
        .with_filename(source_filename.to_string()));
    }
    let optimized = String::from_utf8_lossy(&output.stdout).into_owned();
    if let Ok(path) = std::env::var("MIRE_DUMP_OPT_IR_OUT") {
        let _ = std::fs::write(&path, optimized.as_bytes());
    }
    Ok(optimized)
}

pub(super) fn compile_binary_from_ir(
    ir: &str,
    c_object_files: &[String],
    binary_path: &Path,
    extern_libs: &[(String, String)],
    opt_level: OptLevel,
    source_filename: &str,
    link_crypto_libs: bool,
) -> Result<()> {
    let c_defs = super::build_support::c_defs();
    if matches!(c_defs.artifact, super::config::LibType::Static) {
        // A static library is an archive of objects, not a linked executable
        // with an `.a` suffix. Compile the generated LLVM IR separately and
        // package it together with the runtime/PAL objects using llvm-ar.
        let ir_object = binary_path.with_extension("mire.ir.o");
        // LLVM owns IR-to-object lowering here. Clang is intentionally not
        // involved in producing the Mire object; PAL/runtime C objects are
        // still compiled separately by the C compiler below.
        let mut compile = Command::new("llc");
        compile
            .arg("-filetype=obj")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        compile.arg("-o").arg(&ir_object);
        // `as_opt_flag` is the single source of truth: llc rejects `-Os`/`-Oz`
        // outright ("invalid optimization level"), so both sites had to agree.
        compile.arg(opt_level.as_opt_flag());
        if let Some(target) = &c_defs.target {
            compile.arg("--target").arg(target);
        }
        let mut child = compile.spawn().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to run llc for static library: {}", err),
            })
            .with_filename(source_filename.to_string())
        })?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(ir.as_bytes()).map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    span: crate::error::Span::unknown(),
                    message: format!("Failed to stream IR into llc: {}", err),
                })
                .with_filename(source_filename.to_string())
            })?;
        }
        let status = child.wait().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to wait for llc static library: {}", err),
            })
            .with_filename(source_filename.to_string())
        })?;
        if !status.success() {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("llc failed while compiling static library ({status})"),
            })
            .with_filename(source_filename.to_string()));
        }

        // The destination may be a stale executable from an older compiler
        // version; llvm-ar otherwise tries to update it as an archive.
        let _ = std::fs::remove_file(binary_path);
        let mut archive = Command::new("llvm-ar");
        archive.arg("rcs").arg(binary_path).arg(&ir_object);
        for object in c_object_files {
            archive.arg(object);
        }
        let status = archive.status().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to run llvm-ar: {}", err),
            })
            .with_filename(source_filename.to_string())
        })?;
        let _ = std::fs::remove_file(&ir_object);
        if !status.success() {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!(
                    "llvm-ar failed while creating {} ({status})",
                    binary_path.display()
                ),
            })
            .with_filename(source_filename.to_string()));
        }
        return Ok(());
    }

    if matches!(c_defs.artifact, super::config::LibType::Shared) {
        // A shared library has no process startup objects. Lower Mire IR with
        // LLVM and link the resulting object with the C PAL/runtime objects.
        let is_none_tier = matches!(c_defs.runtime, super::config::RuntimeTier::None);
        let ir_object = binary_path.with_extension("mire.ir.o");
        lower_ir_to_object(
            ir,
            &ir_object,
            c_defs.target.as_deref(),
            c_defs.cpu.as_deref(),
            opt_level,
            source_filename,
            true,
        )?;

        let mut linker = Command::new("ld.lld");
        linker.arg("-shared").arg("-o").arg(binary_path);
        let tcc_path = build_support::find_tcc().unwrap_or_else(|| "tcc".to_string());
        add_tcc_library_search_paths(&tcc_path, &mut linker);
        linker.arg(&ir_object);
        for object in c_object_files {
            linker.arg(object);
        }
        if !is_none_tier {
            linker.arg("-lm");
            if link_crypto_libs {
                linker.args(["-lssl", "-lcrypto", "-lsodium"]);
            }
        }
        for (lib_name, lib_path) in extern_libs {
            let native = std::path::Path::new(lib_path);
            if native.exists() {
                linker.arg(native);
                if let Some(parent) = native.parent() {
                    linker.arg(format!("-L{}", parent.display()));
                }
            } else if !lib_path.is_empty() {
                let clean_name = if lib_name.contains('.') {
                    lib_name.rsplit('.').next().unwrap_or(lib_name)
                } else {
                    lib_name
                };
                linker.arg(format!("-l{}", clean_name));
            }
        }
        for lib in &c_defs.libs {
            linker.arg(format!("-l{}", lib.trim_start_matches("lib")));
        }
        let output = linker.output().map_err(|err| {
            MireError::runtime(format!("Failed to run ld.lld: {err}"))
                .with_filename(source_filename.to_string())
        })?;
        let _ = std::fs::remove_file(&ir_object);
        if !output.status.success() {
            return Err(MireError::runtime(format!(
                "ld.lld failed while building shared library:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
            .with_filename(source_filename.to_string()));
        }
        return Ok(());
    }

    // Lower Mire IR independently of the native link driver. Tcc is
    // responsible for target-aware CRT/libc/native-library linking.
    let ir_object = binary_path.with_extension("mire.ir.o");
    lower_ir_to_object(
        ir,
        &ir_object,
        c_defs.target.as_deref(),
        c_defs.cpu.as_deref(),
        opt_level,
        source_filename,
        true,
    )?;

    let tcc_path = build_support::find_tcc().ok_or_else(|| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: "libtcc (tcc) not found. Install tcc or set the TCC_PATH environment variable. Native linking requires libtcc."
                .to_string(),
        })
    })?;
    let mut linker = Command::new(&tcc_path);
    let mut seen_objects = std::collections::HashSet::new();

    // Pass the LLVM object and native objects to the target-aware linker.
    linker.arg(&ir_object);
    seen_objects.insert(ir_object.to_string_lossy().into_owned());
    for obj in c_object_files {
        if seen_objects.insert(obj.clone()) {
            linker.arg(obj);
        }
    }
    linker.stdout(Stdio::piped()).stderr(Stdio::piped());

    linker.arg("-o").arg(binary_path);
    linker.arg(opt_level.as_opt_flag());

    let is_none_tier = matches!(c_defs.runtime, super::config::RuntimeTier::None);
    let is_library = matches!(
        c_defs.artifact,
        super::config::LibType::Static | super::config::LibType::Shared
    );
    let is_shared = matches!(c_defs.artifact, super::config::LibType::Shared);

    // For shared libraries, pass -shared and -fPIC to tcc
    if is_shared {
        linker.arg("-shared");
        linker.arg("-fPIC");
    }

    // In freestanding mode (none tier), skip runtime libraries — the program
    // provides its own implementations.  In full/minimal tiers, link the
    // standard runtime dependencies.  Crypto libs (libssl/libcrypto/libsodium)
    // are only linked when PAL files are actually compiled.
    if !is_none_tier {
        // Neither -lm nor the crypto libraries go on the first attempt. Each
        // one costs start-up time in the dynamic loader -- mapping and
        // relocating libm, libmvec, libsodium, libssl and libcrypto, plus
        // whatever libcrypto drags in -- and a program that prints an integer
        // needs none of them. Both are added back only if the link says a
        // symbol is missing.
    }

    // Skip C runtime startup files for libraries (shared/static) and when
    // explicitly requested via nostartfiles.  Libraries don't need a main entry point.
    let needs_nostartfiles = c_defs.nostartfiles || (is_library && !is_shared);
    if needs_nostartfiles {
        // Allow nostartfiles for libraries regardless of tier; for binaries it requires none tier.
        if c_defs.nostartfiles && !is_none_tier && !is_library {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: "`nostartfiles = true` requires `runtime = \"none\"` or library type (static/shared)".to_string(),
            }).with_filename(source_filename.to_string()));
        }
        linker.arg("-nostartfiles");
    }

    // True no-libc: skip standard library (libc, libgcc, etc.)
    if c_defs.nostdlib {
        if !is_none_tier {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: "`nostdlib = true` requires `runtime = \"none\"`".to_string(),
            })
            .with_filename(source_filename.to_string()));
        }
        if !c_defs.nostartfiles {
            return Err(MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: "`nostdlib = true` requires `nostartfiles = true`".to_string(),
            })
            .with_filename(source_filename.to_string()));
        }
        linker.arg("-nostdlib");
    }

    // Pass user-defined cflags
    for flag in &c_defs.cflags {
        linker.arg(flag);
    }

    // Add -ffreestanding for none tier or when nostartfiles is set
    if is_none_tier || c_defs.nostartfiles {
        linker.arg("-ffreestanding");
    }

    linker.arg("-pthread");

    // `-x ir -` applies to subsequent inputs in clang. Reset the language
    // before passing native .so/.a files, otherwise clang parses archives as
    // LLVM text and reports a misleading IR syntax error.
    linker.arg("-x").arg("none");
    for (lib_name, lib_path) in extern_libs {
        let clean_name = if lib_name.contains('.') {
            lib_name.rsplit('.').next().unwrap_or(lib_name)
        } else {
            lib_name
        };
        let explicit_native =
            lib_path.ends_with(".so") || lib_path.ends_with(".dylib") || lib_path.ends_with(".a");
        if explicit_native {
            let native_path = std::path::Path::new(&lib_path);
            if let Some(parent) = native_path.parent() {
                linker.arg(format!("-L{}", parent.display()));
                if lib_path.ends_with(".so") || lib_path.ends_with(".dylib") {
                    // Keep explicit fixture/shared-library dependencies
                    // runnable without LD_LIBRARY_PATH. Relative rpaths are
                    // intentional for project-local test libraries.
                    linker.arg(format!("-Wl,-rpath,{}", parent.display()));
                }
            }
            // Pass the library path directly. `tcc` does not understand the
            // GNU `-l:filename` syntax used previously, and its `-l` search
            // does not reliably find exact fixture names such as `kioto.so`.
            linker.arg(native_path);
        } else if lib_path.as_str() != clean_name && !lib_path.is_empty() {
            linker.arg(lib_path);
        }
        if !explicit_native {
            linker.arg("-l");
            linker.arg(clean_name);
        }
    }

    for lib in super::build_support::c_defs().libs.iter() {
        let clean_name = if lib.contains('.') {
            lib.rsplit('.').next().unwrap_or(lib)
        } else {
            lib
        };
        linker.arg("-l");
        linker.arg(clean_name);
    }

    // Some tcc builds refuse to mix an LLVM-emitted object with their own
    // startup objects because the two disagree on the `.note.GNU-stack`
    // section flags ("section type conflict: .note.GNU-stack 01 <> 07").
    // Neither the compile flags nor -Wl,-z,noexecstack change the note; only
    // dropping the section does, and tcc's linker emits no GNU_STACK program
    // header of its own, so nothing is lost. Retrying once keeps the normal
    // path free of an extra process.
    let link_args = linker
        .get_args()
        .map(|a| a.to_os_string())
        .collect::<Vec<_>>();
    let mut output = run_tcc_link(&tcc_path, &link_args)?;
    if link_failed_on_gnu_stack_note(&output) {
        output = retry_link_without_gnu_stack_note(&tcc_path, &link_args, &ir_object)?;
    }
    // Only now add the crypto libraries, and only if the link without them
    // failed for want of a crypto symbol. `link_crypto_libs` is the caller's
    // guess that some PAL object might need them; a link that succeeds
    // without them has proved the guess wrong.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut extra: Vec<&str> = Vec::new();
        if stderr.contains("undefined symbol") {
            // Ask which library could define what is missing rather than
            // guessing: a real error leaves both lists empty and is reported
            // unchanged below.
            if missing_symbols_resolve_to_libm(&stderr) {
                extra.push("-lm");
            }
            if link_crypto_libs {
                extra.extend(crypto_libs_for(&stderr));
            }
        }
        if !extra.is_empty() {
            let mut retry = link_args.clone();
            retry.extend(extra.iter().map(|lib| std::ffi::OsString::from(*lib)));
            output = run_tcc_link(&tcc_path, &retry)?;
        }
    }

    let _ = std::fs::remove_file(&ir_object);
    if !output.status.success() {
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::new(1, 1),
            message: format!(
                "tcc failed building `{}` (source: {}) with status {}.\nstdout:\n{}\nstderr:\n{}",
                binary_path.display(),
                source_filename,
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        })
        .with_filename(source_filename.to_string()));
    }

    // Give the finished binary the same identity a rustc-built one carries, so
    // `file` reports a BuildID and a bug report can be matched to a build.
    // Best effort: an artifact this cannot rewrite (an `ar` archive, a
    // non-ELF output) is not a reason to fail a link that succeeded.
    if let Some(id) = crate::avens::build_id::embed_in(binary_path) {
        if std::env::var("MIRE_QUIET").is_err() {
            eprintln!("build-id: {}", crate::avens::build_id::to_hex(&id));
        }
    }

    Ok(())
}

fn run_tcc_link(tcc_path: &str, args: &[std::ffi::OsString]) -> Result<std::process::Output> {
    Command::new(tcc_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to run tcc: {}", err),
            })
        })
}

/// Version tags that glibc's libm stamps onto the symbols it provides.
///
/// Matching on the tag rather than on symbol names is what separates "needs
/// libm" from "some other library is missing": an undefined `sqrt` says
/// nothing about which library should define it, but `@GLIBC_2.29` does,
/// because every versioned symbol at or above 2.29 lives in libm/libmvec and
/// the base 2.2.5 set is plain libc.
fn missing_symbols_resolve_to_libm(link_error: &str) -> bool {
    if !link_error.contains("undefined symbol") {
        return false;
    }
    link_error
        .lines()
        .filter(|line| line.contains("undefined symbol"))
        .any(|line| {
            libm_version_tags().any(|tag| line.contains(tag)) || line_names_libm_symbol(line)
        })
}

/// Math functions that exist in libm.
///
/// tcc, the default linker, does not stamp undefined symbols with `@GLIBC_*`
/// version tags the way ld does: it reports a bare `undefined symbol 'sinh'`
/// and nothing else. The function name is then the only signal that libm (and
/// not some other library) is what is missing. This list is the set
/// `core/math.c` forwards to libc/libm; the C89 members (`sin`, `sqrt`, ...)
/// are also provided by glibc's libc, so a program using only those still
/// links without -lm, but a tcc build that reaches the C99 members
/// (`sinh`/`erf`/`tgamma`/...) needs it.
const LIBM_SYMBOLS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "sqrt", "cbrt", "pow", "exp", "exp2", "expm1", "log", "log2", "log10", "log1p", "erf",
    "erfc", "tgamma", "lgamma", "hypot", "fabs", "fmod", "remainder", "fma", "copysign", "frexp",
    "ldexp", "nextafter", "modf", "floor", "ceil", "trunc", "llround", "fmax",
];

/// Pull the symbol name out of a linker's `undefined symbol 'NAME'` diagnostic
/// and report whether libm defines it.
fn line_names_libm_symbol(line: &str) -> bool {
    let Some(start) = line.find('\'') else {
        return false;
    };
    let rest = &line[start + 1..];
    let Some(end) = rest.find('\'') else {
        return false;
    };
    LIBM_SYMBOLS.contains(&&rest[..end])
}

fn libm_version_tags() -> impl Iterator<Item = &'static str> {
    // libm gained versioned symbols across these releases; the first is
    // libmvec, the last covers everything since.
    ["@GLIBC_2.35", "@GLIBC_2.34", "@GLIBC_2.32", "@GLIBC_2.29"]
        .into_iter()
}

/// Which libraries the crypto PAL ends up needing, and how to recognise each.
///
/// Split rather than "add all three" because the cost is very uneven.
/// libsodium depends on nothing but libc; libcrypto pulls in libz, libzstd and
/// four brotli objects, and every one of those has to be mapped and relocated
/// before `main` runs. Linking -lcrypto for a program whose only undefined
/// crypto symbol is `sodium_init` costs about 2 ms of start-up for nothing.
///
/// `crypto_` is deliberately not a marker: libsodium exports `crypto_hash_*`
/// and `crypto_sign_*` alongside the `sodium_*` names.
fn crypto_libs_for(link_error: &str) -> Vec<&'static str> {
    let mut libs = Vec::new();
    if ["sodium_", "randombytes_", "argon2_"]
        .iter()
        .any(|marker| link_error.contains(marker))
    {
        libs.push("-lsodium");
    }
    if ["SSL_", "EVP_", "OPENSSL_", "X509_", "BN_"]
        .iter()
        .any(|marker| link_error.contains(marker))
    {
        libs.push("-lssl");
        libs.push("-lcrypto");
    }
    libs
}

fn link_failed_on_gnu_stack_note(output: &std::process::Output) -> bool {
    if output.status.success() {
        return false;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.contains("section type conflict") && stderr.contains(".note.GNU-stack")
}

/// Drop `.note.GNU-stack` from the IR object and run the same link again.
fn retry_link_without_gnu_stack_note(
    tcc_path: &str,
    link_args: &[std::ffi::OsString],
    ir_object: &Path,
) -> Result<std::process::Output> {
    if let Err(err) = strip_gnu_stack_note(ir_object) {
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!(
                "tcc rejected the LLVM object over .note.GNU-stack and the section \
                 could not be removed: {}",
                err
            ),
        }));
    }
    run_tcc_link(tcc_path, link_args)
}

fn strip_gnu_stack_note(object: &Path) -> std::io::Result<()> {
    for tool in ["llvm-objcopy", "objcopy"] {
        let ran = Command::new(tool)
            .arg("--remove-section=.note.GNU-stack")
            .arg(object)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(status) = ran
            && status.success()
        {
            return Ok(());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "neither llvm-objcopy nor objcopy could remove the section",
    ))
}

fn lower_ir_to_object(
    ir: &str,
    output: &Path,
    target: Option<&str>,
    cpu: Option<&str>,
    opt_level: OptLevel,
    source_filename: &str,
    position_independent: bool,
) -> Result<()> {
    let mut command = Command::new("llc");
    command
        .arg("-filetype=obj")
        .arg("-o")
        .arg(output)
        .arg(opt_level.as_opt_flag())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if position_independent {
        command.arg("-relocation-model=pic");
    }
    if let Some(target) = target {
        command.arg("-mtriple").arg(target);
    }
    // Instruction selection. Without this LLVM stays on its generic x86-64
    // baseline, so a build never emits andn/shrx/rorx/lzcnt or any vector
    // instruction even on a host that has them. The C leg of the benchmark
    // suite compiles with -march=native; matching it is the only way the two
    // numbers mean the same thing.
    if let Some(cpu) = cpu {
        command.arg("-mcpu").arg(cpu);
    }
    let mut child = command.spawn().map_err(|err| {
        MireError::runtime(format!("Failed to run llc: {err}"))
            .with_filename(source_filename.to_string())
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(ir.as_bytes()).map_err(|err| {
            MireError::runtime(format!("Failed to stream LLVM IR into llc: {err}"))
                .with_filename(source_filename.to_string())
        })?;
    }
    drop(child.stdin.take());
    let output_result = child.wait_with_output().map_err(|err| {
        MireError::runtime(format!("Failed to wait for llc: {err}"))
            .with_filename(source_filename.to_string())
    })?;
    if !output_result.status.success() {
        return Err(MireError::runtime(format!(
            "llc failed for {}:\n{}",
            source_filename,
            String::from_utf8_lossy(&output_result.stderr).trim()
        ))
        .with_filename(source_filename.to_string()));
    }
    Ok(())
}

/// Adds the host toolchain's library directories when `ld.lld` is used
/// directly. Tcc normally supplies these implicitly; bypassing Tcc means
/// that common libraries such as sqlite3 would otherwise be invisible to the
/// linker even though they are installed and available to normal C builds.
fn add_tcc_library_search_paths(tcc_path: &str, linker: &mut Command) {
    let output = match Command::new(tcc_path).arg("-print-search-dirs").output() {
        Ok(output) if output.status.success() => output,
        _ => return,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut in_libs = false;
    for line in text.lines() {
        if line.starts_with("libraries:") {
            in_libs = true;
            continue;
        }
        if in_libs {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("libtcc1:") || trimmed.starts_with("crt:")
            {
                in_libs = false;
                continue;
            }
            if !trimmed.is_empty() {
                linker.arg(format!("-L{trimmed}"));
            }
        }
    }
}

#[allow(dead_code)]
pub(super) fn llvm_version() -> Result<String> {
    let output = Command::new("llvm-config")
        .arg("--version")
        .output()
        .map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Failed to run llvm-config: {}", err),
            })
        })?;
    if !output.status.success() {
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: "llvm-config --version failed".to_string(),
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
