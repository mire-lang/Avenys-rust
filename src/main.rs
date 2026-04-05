use mire::analyze_program;
use mire::compile_file_with_avenys;
use mire::default_output_dir;
use mire::lexer::tokenize;
use mire::load_project_manifest;
use mire::parser::parse;
use mire::project_manifest_path;
use mire::write_lock_file;
use mire::BuildMode;
use mire::BuildOptions;
use mire::MireError;
use mire::MireManifest;
use mire::MireProject;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Clone)]
struct CommonOptions {
    debug: bool,
    show_tokens: bool,
    show_ast: bool,
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
        "init" => init_command(&cwd, &args[2..]),
        "run" | "avenys" => run_command(&cwd, &args[2..]),
        "build" => build_command(&cwd, &args[2..]),
        "test" => test_command(&cwd, &args[2..]),
        "bench" => bench_command(&cwd, &args[2..]),
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
    };
    let build = compile_file_with_avenys(&path, &options)?;
    if common.debug {
        println!("[AVENYS] binary: {}", build.binary_path.display());
        println!("[AVENYS] ir: {}", build.ir_path.display());
        println!("[AVENYS] opt-ir: {}", build.optimized_ir_path.display());
    }
    let status = Command::new(&build.binary_path)
        .status()
        .map_err(runtime_err)?;
    Ok(status.code().unwrap_or(0))
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
        for entry in fs::read_dir(&test_root).map_err(runtime_err)? {
            let entry = entry.map_err(runtime_err)?;
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) == Some("mire") {
                files.push(path);
            }
        }
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

fn bench_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let (common, file, _output) = parse_command_options(cwd, args)?;
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

    if files.is_empty() {
        return Err(runtime_msg("No .mire benchmark files found"));
    }

    println!("Running {} benchmarks...\n", files.len());

    let mut results = Vec::new();
    for file in files {
        let name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        inspect_source(&file, &common)?;
        let options = BuildOptions {
            mode: BuildMode::Release,
            debug_dump: false,
            output: Some(default_binary_path(&file, false)),
        };

        let start = std::time::Instant::now();
        let build = compile_file_with_avenys(&file, &options)?;
        let compile_time = start.elapsed();

        let start = std::time::Instant::now();
        let output = Command::new(&build.binary_path)
            .output()
            .map_err(runtime_err)?;
        let run_time = start.elapsed();

        let status = output.status.code().unwrap_or(1);
        results.push((name.to_string(), compile_time, run_time, status));

        if status == 0 {
            println!(
                "  BENCH {}: compile={:?} run={:?}",
                name, compile_time, run_time
            );
        } else {
            println!("  FAIL {} (status={})", name, status);
        }
    }

    println!();
    println!("Benchmark Summary:");
    for (name, compile_time, run_time, status) in results {
        if status == 0 {
            println!(
                "  {}: {} compile, {} run",
                name,
                format_duration(compile_time),
                format_duration(run_time)
            );
        }
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

    let build_dir = cwd.join("build");
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir).map_err(runtime_err)?;
        println!("Cleaned {}", build_dir.display());
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
    println!("  Output dir: bin/debug or bin/release");

    Ok(0)
}

fn init_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let project_name = args
        .first()
        .cloned()
        .or_else(|| {
            cwd.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| "mire-app".to_string());

    let manifest_path = project_manifest_path(cwd);
    if manifest_path.exists() {
        return Err(runtime_msg("project.toml already exists in this directory"));
    }

    fs::create_dir_all(cwd.join("code")).map_err(runtime_err)?;
    fs::create_dir_all(cwd.join("tests")).map_err(runtime_err)?;
    fs::create_dir_all(cwd.join("bin").join("debug")).map_err(runtime_err)?;
    fs::create_dir_all(cwd.join("bin").join("release")).map_err(runtime_err)?;

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
    write_lock_file(cwd, &manifest, BuildMode::Release)?;

    let main_path = cwd.join(&entry);
    if !main_path.exists() {
        fs::write(
            &main_path,
            "pub fn main: () >\n    use dasu(Hello from Mire)\n<\n",
        )
        .map_err(runtime_err)?;
    }

    let smoke_test = cwd.join("tests").join("smoke.mire");
    if !smoke_test.exists() {
        fs::write(&smoke_test, "use dasu(smoke ok)\n").map_err(runtime_err)?;
    }

    println!("project.toml");
    println!("code/main.mire");
    println!("tests/smoke.mire");
    println!("bin/debug/");
    println!("bin/release/");
    Ok(0)
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

    let mut program = parse(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if common.show_ast {
        println!("=== Parser ===");
        println!("Parsed {} statements\n", program.statements.len());
    }

    analyze_program(&mut program).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
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
            }
            "--show-tokens" => common.show_tokens = true,
            "--show-ast" => common.show_ast = true,
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

fn print_help() {
    println!("Mire Avenys Compiler - CLI Commands");
    println!();
    println!("Usage: mire <command> [options] [file]");
    println!();
    println!("Commands:");
    println!("  init [name]      Initialize a new Mire project");
    println!("  run [file]      Compile and run a Mire file");
    println!("  build [file]     Compile to binary without running");
    println!("  test [path]     Run test files");
    println!("  bench [path]    Run benchmark tests");
    println!("  clean           Clean build artifacts");
    println!("  info            Show compiler and system info");
    println!("  version         Show version information");
    println!();
    println!("Options:");
    println!("  -d, --debug     Enable debug mode (show tokens, AST)");
    println!("  -o, --output    Specify output path");
    println!("  --show-tokens   Display tokens");
    println!("  --show-ast      Display AST");
    println!("  -O0, -O2, -O3   Optimization level");
    println!("  -h, --help      Show this help");
}
