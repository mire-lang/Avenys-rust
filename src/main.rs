use mire::BuildMode;
use mire::BuildOptions;
use mire::MireError;
use mire::MireManifest;
use mire::MireProject;
use mire::analyze_program;
use mire::compile_file_with_avenys;
use mire::default_output_dir;
use mire::lexer::tokenize;
use mire::load_program_from_file;
use mire::load_project_manifest;
use mire::parser::parse;
use mire::project_manifest_path;
use mire::write_lock_file;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct CommonOptions {
    debug: bool,
    show_tokens: bool,
    show_ast: bool,
    show_semantic: bool,
}

#[derive(Debug, Clone, Default)]
struct RunStatsOptions {
    show_ms: bool,
    show_memory: bool,
    show_cpu: bool,
}

#[derive(Debug, Clone)]
struct DebugOptions {
    file: Option<String>,
    show_tokens: bool,
    show_ast: bool,
    show_log: bool,
    emit_ir_only: bool,
    run_binary: bool,
}

#[derive(Debug, Clone)]
struct BenchOptions {
    common: CommonOptions,
    compare_python: bool,
    repeat: usize,
    warmup: usize,
    timeout: Duration,
    filter: Option<String>,
    write_report: bool,
}

#[derive(Debug, Clone)]
struct ProcessOutcome {
    elapsed: Duration,
    metric: Option<Duration>,
    status_code: Option<i32>,
    timed_out: bool,
}

#[derive(Debug, Clone, Default)]
struct ProcessStats {
    wall: Duration,
    cpu: Option<Duration>,
    max_rss_bytes: Option<u64>,
    status_code: Option<i32>,
}

#[derive(Debug, Clone)]
struct BenchmarkResult {
    name: String,
    compile_time: Duration,
    mire_run_time: Option<Duration>,
    mire_status: Option<i32>,
    mire_timed_out: bool,
    python_run_time: Option<Duration>,
    python_status: Option<i32>,
    python_timed_out: bool,
    note: Option<String>,
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("{}", err.format_color());
            ExitCode::from(1)
        }
    }
}

fn run_cli() -> Result<i32, MireError> {
    let args: Vec<String> = env::args().collect();
    let cwd = env::current_dir().map_err(runtime_err)?;

    if args.len() <= 1 {
        print_help();
        return Ok(1);
    }

    match args[1].as_str() {
        "new" => new_command(&cwd, &args[2..]),
        "init" => init_command(&cwd, &args[2..]),
        "run" | "avenys" => run_command(&cwd, &args[2..]),
        "build" => build_command(&cwd, &args[2..]),
        "test" => test_command(&cwd, &args[2..]),
        "bench" => bench_command(&cwd, &args[2..]),
        "debug" => debug_command(&cwd, &args[2..]),
        "clean" => clean_command(&cwd, &args[2..]),
        "info" => info_command(&cwd, &args[2..]),
        "version" | "--version" => {
            println!("Mire Avenys Compiler v0.1.0");
            println!("Built with LLVM");
            Ok(0)
        }
        "--help" | "-h" | "help" => {
            print_help();
            Ok(0)
        }
        legacy if !legacy.starts_with('-') => run_command(&cwd, &args[1..]),
        _ => {
            print_help();
            Ok(1)
        }
    }
}

fn run_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (common, file, output, stats_options) = parse_run_options(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    inspect_source(&path, &common)?;
    let options = BuildOptions {
        mode: if common.debug {
            BuildMode::Debug
        } else {
            BuildMode::Release
        },
        debug_dump: common.debug,
        output: output.or_else(|| Some(default_binary_path(&path, common.debug))),
        emit_binary: true,
        persist_ir: false,
    };
    let build = compile_file_with_avenys(&path, &options)?;
    if common.debug {
        println!("[AVENYS] binary: {}", build.binary_path.display());
        println!("[AVENYS] ir: <memory>");
        println!("[AVENYS] opt-ir: <memory>");
    }
    let stats = run_process_with_stats(Command::new(&build.binary_path))?;
    print_run_stats(&stats_options, &stats);
    Ok(stats.status_code.unwrap_or(0))
}

fn build_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (common, file, output) = parse_command_options(cwd, args)?;
    let path = resolve_source_path(cwd, file)?;
    inspect_source(&path, &common)?;

    let options = BuildOptions {
        mode: if common.debug {
            BuildMode::Debug
        } else {
            BuildMode::Release
        },
        debug_dump: common.debug,
        output: output.or_else(|| Some(default_binary_path(&path, common.debug))),
        emit_binary: true,
        persist_ir: false,
    };
    let build = compile_file_with_avenys(&path, &options)?;
    println!("{}", build.binary_path.display());

    if let Some(manifest) = ensure_manifest(cwd, &path)? {
        write_lock_file(cwd, &manifest, options.mode)?;
    }

    Ok(0)
}

fn test_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (common, file, _output) = parse_command_options(cwd, args)?;
    let test_root = if let Some(file) = file {
        PathBuf::from(file)
    } else {
        cwd.join("tests")
    };

    let mut files = Vec::new();
    if test_root.is_file() {
        files.push(test_root);
    } else if test_root.is_dir() {
        collect_mire_files_recursive(&test_root, &mut files)?;
        files.sort();
    }

    if files.is_empty() {
        return Err(runtime_msg("No .mire tests found"));
    }

    let mut failed = 0;
    for file in files {
        inspect_source(&file, &common)?;
        let options = BuildOptions {
            mode: if common.debug {
                BuildMode::Debug
            } else {
                BuildMode::Release
            },
            debug_dump: common.debug,
            output: Some(default_binary_path(&file, common.debug)),
            emit_binary: true,
            persist_ir: false,
        };
        let build = compile_file_with_avenys(&file, &options)?;
        let code = Command::new(&build.binary_path)
            .status()
            .map_err(runtime_err)?
            .code()
            .unwrap_or(1);

        if code == 0 {
            println!("PASS {}", file.display());
        } else {
            println!("FAIL {} ({})", file.display(), code);
            failed += 1;
        }
    }

    Ok(if failed == 0 { 0 } else { 1 })
}

fn collect_mire_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), MireError> {
    for entry in fs::read_dir(dir).map_err(runtime_err)? {
        let entry = entry.map_err(runtime_err)?;
        let path = entry.path();
        if path.is_dir() {
            if should_descend_test_dir(&path) {
                collect_mire_files_recursive(&path, files)?;
            }
        } else if path.extension().and_then(|v| v.to_str()) == Some("mire") {
            files.push(path);
        }
    }
    Ok(())
}

fn should_descend_test_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return true;
    };
    !matches!(name, "broken_mire" | "error" | "test_proyet_mire_cli")
}

fn bench_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (bench_options, file) = parse_bench_options(cwd, args)?;
    let bench_root = if let Some(file) = file {
        PathBuf::from(file)
    } else {
        cwd.join("benchmarks")
    };

    let mut files = Vec::new();
    if bench_root.is_file() {
        files.push(bench_root);
    } else if bench_root.is_dir() {
        for entry in fs::read_dir(&bench_root).map_err(runtime_err)? {
            let entry = entry.map_err(runtime_err)?;
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) == Some("mire") {
                files.push(path);
            }
        }
        files.sort();
    }

    if let Some(filter) = &bench_options.filter {
        files.retain(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.contains(filter))
        });
    }

    if files.is_empty() {
        return Err(runtime_msg("No .mire benchmark files found"));
    }

    println!(
        "Running {} benchmarks (repeat={}, warmup={}, timeout={}ms, compare_python={})...\n",
        files.len(),
        bench_options.repeat,
        bench_options.warmup,
        bench_options.timeout.as_millis(),
        bench_options.compare_python
    );

    let mut results = Vec::new();
    for file in files {
        let name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        if let Err(err) = inspect_source(&file, &bench_options.common) {
            let result = BenchmarkResult {
                name: name.to_string(),
                compile_time: Duration::default(),
                mire_run_time: None,
                mire_status: None,
                mire_timed_out: false,
                python_run_time: None,
                python_status: None,
                python_timed_out: false,
                note: Some(err.to_string()),
            };
            print_benchmark_row(&result);
            results.push(result);
            continue;
        }

        let build_options = BuildOptions {
            mode: BuildMode::Release,
            debug_dump: false,
            output: Some(default_binary_path(&file, false)),
            emit_binary: true,
            persist_ir: false,
        };

        let start = std::time::Instant::now();
        let build = match compile_file_with_avenys(&file, &build_options) {
            Ok(build) => build,
            Err(err) => {
                let result = BenchmarkResult {
                    name: name.to_string(),
                    compile_time: start.elapsed(),
                    mire_run_time: None,
                    mire_status: None,
                    mire_timed_out: false,
                    python_run_time: None,
                    python_status: None,
                    python_timed_out: false,
                    note: Some(err.to_string()),
                };
                print_benchmark_row(&result);
                results.push(result);
                continue;
            }
        };
        let compile_time = start.elapsed();

        let mire_run = benchmark_command(
            || {
                let mut command = Command::new(&build.binary_path);
                command.current_dir(cwd);
                command
            },
            bench_options.timeout,
            bench_options.warmup,
            bench_options.repeat,
        )?;

        let python_pair = if bench_options.compare_python {
            find_python_benchmark(&file)
        } else {
            None
        };
        let python_run = if let Some(py_file) = python_pair.as_ref() {
            Some(benchmark_command(
                || {
                    let mut command = Command::new("python3");
                    command.arg(py_file);
                    command.current_dir(cwd);
                    command
                },
                bench_options.timeout,
                bench_options.warmup,
                bench_options.repeat,
            )?)
        } else {
            None
        };

        let result = BenchmarkResult {
            name: name.to_string(),
            compile_time,
            mire_run_time: Some(process_duration(&mire_run)),
            mire_status: mire_run.status_code,
            mire_timed_out: mire_run.timed_out,
            python_run_time: python_run.as_ref().map(process_duration),
            python_status: python_run.as_ref().and_then(|run| run.status_code),
            python_timed_out: python_run.as_ref().is_some_and(|run| run.timed_out),
            note: None,
        };

        print_benchmark_row(&result);
        results.push(result);
    }

    print_benchmark_summary(&results);
    if bench_options.write_report {
        let report_path = cwd.join("benchmarks").join("LATEST_RESULTS.md");
        write_benchmark_report(&report_path, &results)?;
        println!("\nWrote {}", report_path.display());
    }

    Ok(0)
}

fn debug_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let options = parse_debug_options(cwd, args)?;
    let path = resolve_source_path(cwd, options.file.clone())?;
    let source = fs::read_to_string(&path).map_err(runtime_err)?;
    let tokens = tokenize(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if options.show_tokens {
        println!("=== Lexer ===");
        for token in &tokens {
            println!("{:?}", token);
        }
        println!("\nTokens: {}\n", tokens.len());
    }
    let program = parse(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if options.show_ast {
        println!("=== Parser ===");
        println!("{:#?}", program);
        println!("\nStatements: {}\n", program.statements.len());
    }
    let mut expanded_program = load_program_from_file(&path)?;
    let _semantic = analyze_program(&mut expanded_program, &source)?;

    let build_options = BuildOptions {
        mode: BuildMode::Debug,
        debug_dump: true,
        output: Some(default_binary_path(&path, true)),
        emit_binary: !options.emit_ir_only,
        persist_ir: true,
    };
    let build = compile_file_with_avenys(&path, &build_options)?;
    if options.show_log {
        println!("=== Debug Snapshot ===");
        println!("File: {}", path.display());
        println!("Tokens: {}", tokens.len());
        println!("Statements: {}", program.statements.len());
        if !options.emit_ir_only {
            println!("Binary: {}", build.binary_path.display());
        }
        if let Some(ir_path) = &build.ir_path {
            println!("IR: {}", ir_path.display());
        }
        if let Some(ir_path) = &build.optimized_ir_path {
            println!("Optimized IR: {}", ir_path.display());
        }
    } else if options.emit_ir_only {
        let ir_path = build
            .optimized_ir_path
            .as_ref()
            .ok_or_else(|| runtime_msg("Debug IR was not persisted to disk"))?;
        println!("{}", ir_path.display());
    } else {
        println!("{}", build.binary_path.display());
    }

    if options.run_binary && !options.emit_ir_only {
        let stats = run_process_with_stats(Command::new(&build.binary_path))?;
        if options.show_log {
            println!("wall_ms {:.3}", stats.wall.as_secs_f64() * 1000.0);
            if let Some(cpu) = stats.cpu {
                println!("cpu_ms {:.3}", cpu.as_secs_f64() * 1000.0);
            }
        }
        return Ok(stats.status_code.unwrap_or(0));
    }

    Ok(0)
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    if secs > 0 {
        format!("{}.{:03}s", secs, millis)
    } else {
        format!("{}.{:03}ms", millis, d.subsec_micros() % 1000)
    }
}

fn clean_command(cwd: &Path, _args: &[String]) -> Result<i32, MireError> {
    let bin_dir = cwd.join("bin");
    if bin_dir.exists() {
        fs::remove_dir_all(&bin_dir).map_err(runtime_err)?;
        println!("Cleaned {}", bin_dir.display());
    }

    for dir in [cwd.join("debug"), cwd.join("release"), cwd.join(".cache")] {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(runtime_err)?;
            println!("Cleaned {}", dir.display());
        }
    }

    println!("Build artifacts cleaned");
    Ok(0)
}

fn info_command(cwd: &Path, _args: &[String]) -> Result<i32, MireError> {
    println!("Mire Avenys Compiler Information");
    println!("================================");
    println!();

    let llvm_version = Command::new("llvm-config")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Not found".to_string());
    println!("LLVM Version: {}", llvm_version);

    let clang_version = Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            output.lines().next().unwrap_or("Not found").to_string()
        })
        .unwrap_or_else(|_| "Not found".to_string());
    println!("Clang: {}", clang_version);

    if let Ok(manifest) = load_project_manifest(cwd) {
        if let Some(m) = manifest {
            println!();
            println!("Project: {} v{}", m.project.name, m.project.version);
            println!("Entry: {}", m.project.entry);
        }
    }

    println!();
    println!("Build Configuration:");
    println!("  Release optimizations: -O3");
    println!("  Debug mode: -O0");
    println!("  Project output dir: bin/debug or bin/release");
    println!("  Project incremental cache: bin/.cache");

    Ok(0)
}

fn init_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    if args.is_empty() {
        return create_project_in_dir(cwd, cwd);
    }

    let target = cwd.join(&args[0]);
    create_project_in_dir(&target, &target)
}

fn new_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let project_name = args
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let target = cwd.join(&project_name);
    create_project_in_dir(&target, &target)
}

fn create_project_in_dir(project_dir: &Path, manifest_dir: &Path) -> Result<i32, MireError> {
    let project_name = manifest_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("default")
        .to_string();

    fs::create_dir_all(project_dir).map_err(runtime_err)?;

    let manifest_path = project_manifest_path(manifest_dir);
    if manifest_path.exists() {
        return Err(runtime_msg("project.toml already exists in this directory"));
    }

    fs::create_dir_all(manifest_dir.join("code")).map_err(runtime_err)?;
    fs::create_dir_all(manifest_dir.join("tests")).map_err(runtime_err)?;
    fs::create_dir_all(manifest_dir.join("bin").join("debug")).map_err(runtime_err)?;
    fs::create_dir_all(manifest_dir.join("bin").join("release")).map_err(runtime_err)?;

    let entry = "code/main.mire".to_string();
    let manifest = MireManifest {
        project: MireProject {
            name: project_name,
            version: "0.1.0".to_string(),
            entry: entry.clone(),
        },
    };
    let manifest_raw = toml::to_string_pretty(&manifest)
        .map_err(|err| runtime_msg(&format!("Could not serialize project.toml: {}", err)))?;
    fs::write(&manifest_path, manifest_raw).map_err(runtime_err)?;
    write_lock_file(manifest_dir, &manifest, BuildMode::Release)?;

    let main_path = manifest_dir.join(&entry);
    if !main_path.exists() {
        fs::write(
            &main_path,
            "pub fn main: () {\n    use dasu(Hello from Mire)\n}\n",
        )
        .map_err(runtime_err)?;
    }

    let smoke_test = manifest_dir.join("tests").join("smoke.mire");
    if !smoke_test.exists() {
        fs::write(&smoke_test, "use dasu(smoke ok)\n").map_err(runtime_err)?;
    }

    println!("{}", manifest_path.display());
    println!("{}", main_path.display());
    println!("{}", smoke_test.display());
    println!("{}", manifest_dir.join("bin").join("debug").display());
    println!("{}", manifest_dir.join("bin").join("release").display());
    Ok(0)
}

fn parse_run_options(
    cwd: &Path,
    args: &[String],
) -> Result<
    (
        CommonOptions,
        Option<String>,
        Option<PathBuf>,
        RunStatsOptions,
    ),
    MireError,
> {
    let (common, file, output) = parse_command_options(cwd, args)?;
    let mut stats = RunStatsOptions::default();
    for arg in args {
        match arg.as_str() {
            "--ms" => stats.show_ms = true,
            "--memory" | "-m" => stats.show_memory = true,
            "--cpu" => stats.show_cpu = true,
            _ => {}
        }
    }
    Ok((common, file, output, stats))
}

fn parse_debug_options(cwd: &Path, args: &[String]) -> Result<DebugOptions, MireError> {
    let mut file = None;
    let mut show_tokens = false;
    let mut show_ast = false;
    let mut show_log = false;
    let mut emit_ir_only = false;
    let mut run_binary = false;

    for arg in args {
        match arg.as_str() {
            "--tokens" | "-t" => show_tokens = true,
            "--ast" | "-p" => show_ast = true,
            "--log" | "-l" => show_log = true,
            "--ir" => emit_ir_only = true,
            "--run" | "-r" => run_binary = true,
            value if !value.starts_with('-') && file.is_none() => file = Some(value.to_string()),
            _ => {}
        }
    }

    if file.is_none() && load_project_manifest(cwd)?.is_none() {
        return Err(runtime_msg(
            "Missing input file and no project.toml entry found",
        ));
    }

    Ok(DebugOptions {
        file,
        show_tokens,
        show_ast,
        show_log,
        emit_ir_only,
        run_binary,
    })
}

fn print_run_stats(options: &RunStatsOptions, stats: &ProcessStats) {
    if options.show_ms {
        println!("wall_ms {:.3}", stats.wall.as_secs_f64() * 1000.0);
    }
    if options.show_cpu {
        if let Some(cpu) = stats.cpu {
            println!("cpu_ms {:.3}", cpu.as_secs_f64() * 1000.0);
        }
    }
    if options.show_memory {
        if let Some(bytes) = stats.max_rss_bytes {
            println!("max_rss {}", format_bytes(bytes));
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let value = bytes as f64;
    if value >= MB {
        format!("{:.2} MB", value / MB)
    } else if value >= KB {
        format!("{:.2} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn run_process_with_stats(mut command: Command) -> Result<ProcessStats, MireError> {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;

        let start = Instant::now();
        let child = command.spawn().map_err(runtime_err)?;
        let pid = child.id() as libc::pid_t;

        loop {
            let mut status = 0;
            let mut usage = MaybeUninit::<libc::rusage>::zeroed();
            let wait_result =
                unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, usage.as_mut_ptr()) };

            if wait_result == pid {
                let usage = unsafe { usage.assume_init() };
                let cpu = timeval_to_duration(usage.ru_utime)
                    .checked_add(timeval_to_duration(usage.ru_stime))
                    .or_else(|| Some(Duration::default()));
                let max_rss_bytes = Some((usage.ru_maxrss as u64) * 1024);
                return Ok(ProcessStats {
                    wall: start.elapsed(),
                    cpu,
                    max_rss_bytes,
                    status_code: decode_wait_status(status),
                });
            }

            if wait_result == -1 {
                return Err(runtime_msg("Could not wait for process"));
            }

            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(not(unix))]
    {
        let status = command.status().map_err(runtime_err)?;
        Ok(ProcessStats {
            wall: Duration::default(),
            cpu: None,
            max_rss_bytes: None,
            status_code: status.code(),
        })
    }
}

#[cfg(unix)]
fn timeval_to_duration(value: libc::timeval) -> Duration {
    Duration::from_secs(value.tv_sec as u64) + Duration::from_micros(value.tv_usec as u64)
}

#[cfg(unix)]
fn decode_wait_status(status: i32) -> Option<i32> {
    if libc::WIFEXITED(status) {
        Some(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        Some(128 + libc::WTERMSIG(status))
    } else {
        None
    }
}

fn inspect_source(path: &Path, common: &CommonOptions) -> Result<(), MireError> {
    let source = fs::read_to_string(path).map_err(runtime_err)?;
    if common.debug {
        println!("[AVENYS] File: {} ({} bytes)", path.display(), source.len());
    }

    let tokens = tokenize(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if common.show_tokens {
        println!("=== Lexer ===");
        for token in &tokens {
            println!("{:?}", token);
        }
        println!("\nTokens: {}\n", tokens.len());
    }

    let program = parse(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if common.show_ast {
        println!("=== Parser ===");
        println!("{:#?}", program);
        println!("\nStatements: {}\n", program.statements.len());
    }

    let mut expanded_program = load_program_from_file(path)?;
    let semantic_model = analyze_program(&mut expanded_program, &source)?;
    if common.show_semantic {
        println!("=== Semantic ===");
        println!("{:#?}\n", semantic_model);
    }
    Ok(())
}

fn parse_command_options(
    cwd: &Path,
    args: &[String],
) -> Result<(CommonOptions, Option<String>, Option<PathBuf>), MireError> {
    let mut common = CommonOptions {
        debug: false,
        show_tokens: false,
        show_ast: false,
        show_semantic: false,
    };
    let mut file = None;
    let mut output = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-d" | "--debug" => {
                common.debug = true;
                common.show_tokens = true;
                common.show_ast = true;
                common.show_semantic = true;
            }
            "--ms" | "--memory" | "-m" | "--cpu" => {}
            "--show-tokens" => common.show_tokens = true,
            "--show-ast" => common.show_ast = true,
            "--show-semantic" => common.show_semantic = true,
            "--engine=runtime" | "--runtime" => {
                return Err(runtime_msg(
                    "The interpreter has been removed from the CLI. Use compiled Avenys mode only.",
                ));
            }
            "--engine=auto" => {
                return Err(runtime_msg(
                    "Auto engine has been removed. Mire now runs as a compiled-only toolchain.",
                ));
            }
            "--engine=avenys" | "--avenys" => {}
            "-o" | "--output" => {
                index += 1;
                let out = args
                    .get(index)
                    .ok_or_else(|| runtime_msg("Missing output path"))?;
                output = Some(PathBuf::from(out));
            }
            arg if !arg.starts_with('-') && file.is_none() => file = Some(arg.to_string()),
            _ => {}
        }
        index += 1;
    }

    if file.is_none() && load_project_manifest(cwd)?.is_none() {
        return Err(runtime_msg(
            "Missing input file and no project.toml entry found",
        ));
    }

    Ok((common, file, output))
}

fn parse_bench_options(
    cwd: &Path,
    args: &[String],
) -> Result<(BenchOptions, Option<String>), MireError> {
    let mut common = CommonOptions {
        debug: false,
        show_tokens: false,
        show_ast: false,
        show_semantic: false,
    };
    let mut file = None;
    let mut compare_python = true;
    let mut repeat = 3usize;
    let mut warmup = 1usize;
    let mut timeout = Duration::from_millis(1500);
    let mut filter = None;
    let mut write_report = true;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-d" | "--debug" => {
                common.debug = true;
                common.show_tokens = true;
                common.show_ast = true;
                common.show_semantic = true;
            }
            "--show-tokens" => common.show_tokens = true,
            "--show-ast" => common.show_ast = true,
            "--show-semantic" => common.show_semantic = true,
            "--compare-python" => compare_python = true,
            "--no-compare-python" => compare_python = false,
            "--no-report" => write_report = false,
            "--repeat" => {
                index += 1;
                repeat = parse_usize_arg(args.get(index), "repeat")?;
            }
            "--warmup" => {
                index += 1;
                warmup = parse_usize_arg(args.get(index), "warmup")?;
            }
            "--timeout-ms" => {
                index += 1;
                timeout = Duration::from_millis(parse_u64_arg(args.get(index), "timeout-ms")?);
            }
            "--filter" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| runtime_msg("Missing filter value"))?;
                filter = Some(value.clone());
            }
            arg if !arg.starts_with('-') && file.is_none() => file = Some(arg.to_string()),
            _ => {}
        }
        index += 1;
    }

    if file.is_none() && !cwd.join("benchmarks").exists() {
        return Err(runtime_msg(
            "Missing benchmark path and no benchmarks/ directory found",
        ));
    }

    Ok((
        BenchOptions {
            common,
            compare_python,
            repeat,
            warmup,
            timeout,
            filter,
            write_report,
        },
        file,
    ))
}

fn resolve_source_path(cwd: &Path, file: Option<String>) -> Result<PathBuf, MireError> {
    if let Some(file) = file {
        return Ok(PathBuf::from(file));
    }

    let manifest =
        load_project_manifest(cwd)?.ok_or_else(|| runtime_msg("Missing project.toml"))?;
    Ok(cwd.join(&manifest.project.entry))
}

fn ensure_manifest(cwd: &Path, source_path: &Path) -> Result<Option<MireManifest>, MireError> {
    if let Some(manifest) = load_project_manifest(cwd)? {
        return Ok(Some(manifest));
    }

    let name = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("mire-app")
        .to_string();
    let manifest = MireManifest {
        project: MireProject {
            name,
            version: "0.1.0".to_string(),
            entry: source_path
                .strip_prefix(cwd)
                .unwrap_or(source_path)
                .display()
                .to_string(),
        },
    };
    let raw = toml::to_string_pretty(&manifest)
        .map_err(|err| runtime_msg(&format!("Could not serialize project.toml: {}", err)))?;
    fs::write(project_manifest_path(cwd), raw).map_err(runtime_err)?;
    Ok(Some(manifest))
}

fn default_binary_path(source_path: &Path, debug: bool) -> PathBuf {
    let mode = if debug {
        BuildMode::Debug
    } else {
        BuildMode::Release
    };
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    default_output_dir(source_path, mode).join(stem)
}

fn runtime_err<E: std::fmt::Display>(err: E) -> MireError {
    MireError::new(mire::ErrorKind::Runtime {
        message: err.to_string(),
    })
}

fn runtime_msg(message: &str) -> MireError {
    MireError::new(mire::ErrorKind::Runtime {
        message: message.to_string(),
    })
}

fn parse_usize_arg(value: Option<&String>, name: &str) -> Result<usize, MireError> {
    let raw = value.ok_or_else(|| runtime_msg(&format!("Missing {}", name)))?;
    raw.parse::<usize>()
        .map_err(|_| runtime_msg(&format!("Invalid {} '{}'", name, raw)))
}

fn parse_u64_arg(value: Option<&String>, name: &str) -> Result<u64, MireError> {
    let raw = value.ok_or_else(|| runtime_msg(&format!("Missing {}", name)))?;
    raw.parse::<u64>()
        .map_err(|_| runtime_msg(&format!("Invalid {} '{}'", name, raw)))
}

fn benchmark_command<F>(
    mut make_command: F,
    timeout: Duration,
    warmup: usize,
    repeat: usize,
) -> Result<ProcessOutcome, MireError>
where
    F: FnMut() -> Command,
{
    for _ in 0..warmup {
        let outcome = run_command_with_timeout(make_command(), timeout)?;
        if outcome.timed_out || outcome.status_code.unwrap_or(1) != 0 {
            return Ok(outcome);
        }
    }

    let mut runs = Vec::with_capacity(repeat.max(1));
    for _ in 0..repeat.max(1) {
        let outcome = run_command_with_timeout(make_command(), timeout)?;
        if outcome.timed_out || outcome.status_code.unwrap_or(1) != 0 {
            return Ok(outcome);
        }
        runs.push(outcome);
    }

    runs.sort_by_key(process_duration);
    Ok(runs[runs.len() / 2].clone())
}

fn process_duration(outcome: &ProcessOutcome) -> Duration {
    outcome.metric.unwrap_or(outcome.elapsed)
}

fn run_command_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutcome, MireError> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let start = Instant::now();
    let mut child = command.spawn().map_err(runtime_err)?;

    loop {
        if child.try_wait().map_err(runtime_err)?.is_some() {
            let output = child.wait_with_output().map_err(runtime_err)?;
            return Ok(ProcessOutcome {
                elapsed: start.elapsed(),
                metric: parse_benchmark_metric(&String::from_utf8_lossy(&output.stdout)),
                status_code: output.status.code(),
                timed_out: false,
            });
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ProcessOutcome {
                elapsed: start.elapsed(),
                metric: None,
                status_code: None,
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn parse_benchmark_metric(output: &str) -> Option<Duration> {
    for key in ["wall_ms", "elapsed_ms", "cpu_ms"] {
        for line in output.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            if name != key {
                continue;
            }
            let Some(value) = parts.next() else { continue };
            if let Ok(ms) = value.parse::<f64>() {
                return Some(Duration::from_secs_f64(ms / 1000.0));
            }
        }
    }
    None
}

fn find_python_benchmark(mire_path: &Path) -> Option<PathBuf> {
    let dir = mire_path.parent()?;
    let stem = mire_path.file_stem()?.to_str()?;
    let direct = dir.join(format!("{stem}.py"));
    if direct.exists() {
        return Some(direct);
    }
    let prefixed = dir.join(format!("py_{stem}.py"));
    if prefixed.exists() {
        return Some(prefixed);
    }
    None
}

fn print_benchmark_row(result: &BenchmarkResult) {
    if let Some(note) = &result.note {
        println!(
            "  {:24} compile {:>10}  error {}",
            result.name,
            format_duration(result.compile_time),
            note.lines().next().unwrap_or("unknown error")
        );
        return;
    }

    let mire = format_process_cell(
        result.mire_run_time,
        result.mire_status,
        result.mire_timed_out,
    );
    let python = format_process_cell(
        result.python_run_time,
        result.python_status,
        result.python_timed_out,
    );
    let speedup = match (result.mire_run_time, result.python_run_time) {
        (Some(mire), Some(python)) if mire.as_nanos() > 0 => {
            format!("{:.2}x", python.as_secs_f64() / mire.as_secs_f64())
        }
        _ => "-".to_string(),
    };

    println!(
        "  {:24} compile {:>10}  mire {:>12}  py {:>12}  speedup {:>8}",
        result.name,
        format_duration(result.compile_time),
        mire,
        python,
        speedup
    );
}

fn print_benchmark_summary(results: &[BenchmarkResult]) {
    println!("\nBenchmark Summary:");
    for result in results {
        let status = if result.mire_timed_out {
            "mire timeout"
        } else if result.note.is_some() {
            "compile/analyze fail"
        } else if result.mire_status.unwrap_or(1) != 0 {
            "mire fail"
        } else if result.python_timed_out {
            "python timeout"
        } else if result.python_status.unwrap_or(0) != 0 {
            "python fail"
        } else {
            "ok"
        };
        println!(
            "  {}: compile {}, mire {}, python {}, status {}",
            result.name,
            format_duration(result.compile_time),
            format_process_cell(
                result.mire_run_time,
                result.mire_status,
                result.mire_timed_out
            ),
            format_process_cell(
                result.python_run_time,
                result.python_status,
                result.python_timed_out
            ),
            status
        );
    }
}

fn write_benchmark_report(path: &Path, results: &[BenchmarkResult]) -> Result<(), MireError> {
    let mut out = String::new();
    out.push_str("# Latest Benchmark Results\n\n");
    out.push_str("| Benchmark | Compile | Mire | Python | Speedup |\n");
    out.push_str("|-----------|---------|------|--------|---------|\n");
    for result in results {
        let speedup = match (result.mire_run_time, result.python_run_time) {
            (Some(mire), Some(python)) if mire.as_nanos() > 0 => {
                format!("{:.2}x", python.as_secs_f64() / mire.as_secs_f64())
            }
            _ => "-".to_string(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            result.name,
            format_duration(result.compile_time),
            result.note.clone().unwrap_or_else(|| {
                format_process_cell(
                    result.mire_run_time,
                    result.mire_status,
                    result.mire_timed_out,
                )
            }),
            format_process_cell(
                result.python_run_time,
                result.python_status,
                result.python_timed_out
            ),
            speedup
        ));
    }
    fs::write(path, out).map_err(runtime_err)
}

fn format_process_cell(run: Option<Duration>, status: Option<i32>, timed_out: bool) -> String {
    if timed_out {
        return "timeout".to_string();
    }
    if status.unwrap_or(0) != 0 {
        return format!("exit {}", status.unwrap_or(-1));
    }
    run.map(format_duration)
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_help() {
    println!("Mire CLI");
    println!();
    println!("Usage: mire <command> [options] [file]");
    println!();
    println!("Basic commands:");
    println!("  run [file] [options]    Compile and run");
    println!("  build [file]            Compile only using the project target/output");
    println!("  new [name]              Create a project, default name: default");
    println!("  debug [file] [options]  Debug compile in LLVM IR mode");
    println!();
    println!("run options:");
    println!("  --ms                 Show wall-clock time in ms");
    println!("  --memory, -m         Show peak process memory");
    println!("  --cpu                Show process CPU time in ms");
    println!();
    println!("debug options:");
    println!("  --ast, -p            Show parsed AST");
    println!("  --tokens, -t         Show lexer tokens");
    println!("  --run, -r            Run the compiled debug binary");
    println!("  --log, -l            Show relevant compile/debug info");
    println!("  --ir                 Emit only LLVM IR");
    println!();
    println!("Other commands still available: test, bench, clean, info, version, init");
}