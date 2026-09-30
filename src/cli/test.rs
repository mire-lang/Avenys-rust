use crate::cli::*;
use mire::error::diagnostic::{Diagnostic, WarningFilter};
use mire::{BuildMode, BuildOptions, ImportMode, MireError, OptLevel, compile_file_with_avenys};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

enum UnitStatus {
    Pass,
    Fail(String),
    Compiled,
}

pub(crate) fn test_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let mut run = true;
    let mut verbose = false;
    let mut jobs: usize = 0;
    let mut paths: Vec<String> = Vec::new();
    let mut opt_level = OptLevel::O0;
    let mut categorize = true;
    let mut show_warn = false;
    let mut position = false;
    let mut write_logs = false;
    let mut no_warn_cats: Vec<String> = Vec::new();
    let mut lib_dir: Option<String> = None;
    let mut cache_dir: Option<String> = None;
    let mut config_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Usage: mire test [paths...] [options]");
                println!();
                println!("Run integration tests, optionally categorized by directory.");
                println!();
                println!("Options:");
                println!("  --no-run            Compile only, skip execution");
                println!("  --verbose, -v       Show per-test results");
                println!("  --no-categorize     Disable directory-based category grouping");
                println!("  --jobs, -j <n>      Parallel compilation jobs (0 = logical CPUs)");
                println!(
                    "  -O, --opt-level <n> Optimization level for test binaries (0,1,2,3,s,z)"
                );
                println!("  -r, --release       Shorthand for --opt-level 3");
                println!("  -d, --debug         Shorthand for --opt-level 0 (default)");
                println!("  --show-warn, --sh-warn  Show warnings (summary by default)");
                println!("  --position, --pos       Show warnings per-file (detailed)");
                println!("  --no-warn <cat>         Suppress warning category (repeatable)");
                println!("  --log                   Write tests/log/<family>/ metrics");
                println!("  --lib-dir <path>        Package directory supplied by Owl");
                println!("  --cache-dir <path>     Incremental cache directory");
                println!("  --config <file>        Owl-generated normalized compiler config");
                println!("  --help, -h          Show this help message");
                return Ok(0);
            }
            "--no-run" => run = false,
            "--verbose" | "-v" => verbose = true,
            "--no-categorize" => categorize = false,
            "--show-warn" | "--sh-warn" => show_warn = true,
            "--position" | "--pos" => position = true,
            "--log" => write_logs = true,
            "--lib-dir" => {
                i += 1;
                lib_dir = Some(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing value for --lib-dir"))?
                        .clone(),
                );
            }
            "--cache-dir" | "--cache" => {
                i += 1;
                cache_dir = Some(
                    args.get(i)
                        .ok_or_else(|| cli_msg("Missing value for --cache-dir"))?
                        .clone(),
                );
            }
            "--config" => {
                i += 1;
                config_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        cli_msg("Missing config path after --config")
                    })?));
            }
            "--no-warn" => {
                i += 1;
                let cat = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing warning category after --no-warn"))?;
                no_warn_cats.push(cat.clone());
            }
            "--jobs" | "-j" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing value for --jobs"))?;
                jobs = value
                    .parse()
                    .map_err(|_| cli_msg("--jobs must be a positive integer"))?;
            }
            "-O" | "--opt-level" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing value for --opt-level"))?;
                match OptLevel::parse(value) {
                    Some(level) => opt_level = level,
                    None => return Err(cli_msg("Invalid opt-level")),
                }
            }
            "-r" | "--release" => opt_level = OptLevel::O3,
            "-d" | "--debug" => opt_level = OptLevel::O0,
            _ => {
                if let Some(val) = args[i].strip_prefix("--jobs=") {
                    jobs = val
                        .parse()
                        .map_err(|_| cli_msg("--jobs must be a positive integer"))?;
                } else {
                    paths.push(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let c_defs = c_defs_for(cwd, config_path.as_deref())?;

    if let Some(path) = lib_dir {
        // Package installation and selection remain Owl responsibilities;
        // this process receives only the resolved search directory.
        unsafe { std::env::set_var("MIRE_LIB_DIR", path) };
    }
    if let Some(path) = cache_dir {
        unsafe { std::env::set_var("MIRE_CACHE_DIR", path) };
    }

    // --- helpers ---------------------------------------------------
    fn read_owl_test_paths(cwd: &Path) -> Vec<(String, PathBuf)> {
        let manifest_paths = [
            cwd.join("owl.toml"),
            cwd.join("Mire.toml"),
            cwd.join("Avenys.toml"),
        ];
        let mut content = String::new();
        for m in &manifest_paths {
            if let Ok(c) = fs::read_to_string(m) {
                content = c;
                break;
            }
        }
        let mut in_section = String::new();
        let mut found: Vec<(String, String)> = Vec::new();
        for raw in content.lines() {
            let line = raw.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_section = line[1..line.len() - 1].to_string();
                continue;
            }
            if in_section == "tests" {
                if let Some(v) = kv_string(line, "path") {
                    found.push(("tests".to_string(), v));
                } else if let Some(v) = kv_string(line, "dirs") {
                    for p in parse_array_value(&v) {
                        if !p.is_empty() {
                            found.push(("dirs".to_string(), p));
                        }
                    }
                } else if let Some((key, val)) = parse_generic_kv(line) {
                    found.push((key, val));
                }
            } else if in_section == "paths"
                && let Some(v) = kv_string(line, "tests")
            {
                found.push(("paths".to_string(), v));
            }
        }
        if found.is_empty() {
            found.push(("tests".to_string(), "tests".to_string()));
        }
        let mut unique = HashSet::new();
        found
            .into_iter()
            .filter_map(|(k, p)| {
                let path = cwd.join(p);
                if unique.insert(path.clone()) {
                    Some((k, path))
                } else {
                    None
                }
            })
            .collect()
    }

    fn kv_string(line: &str, key: &str) -> Option<String> {
        let (actual_key, rest) = line.split_once('=')?;
        if actual_key.trim() != key {
            return None;
        }
        let rest = rest.trim();
        if let Some(stripped) = rest.strip_prefix('"') {
            let end = stripped.find('"')?;
            return Some(stripped[..end].to_string());
        }
        if rest.starts_with('[') {
            let inner = rest.trim_start_matches('[').trim_end_matches(']');
            for part in inner.split(',') {
                let p = part.trim().trim_matches('"').to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
        None
    }

    fn unit_category(base: &Path, file: &Path) -> String {
        let rel = match file.strip_prefix(base) {
            Ok(r) => r,
            Err(_) => return String::new(),
        };
        let comps: Vec<_> = rel.components().collect();
        if comps.len() >= 2 {
            comps[0].as_os_str().to_string_lossy().to_string()
        } else {
            String::new()
        }
    }

    fn find_golden_dirs(root: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(ft) = path.metadata() else {
                    continue;
                };
                if ft.is_dir() {
                    stack.push(path.clone());
                } else if path
                    .file_name()
                    .map(|n| n == "program.mire" || n == "program.mr")
                    .unwrap_or(false)
                {
                    let dir = path.parent().unwrap();
                    let has_expect = dir.join("stdout.txt").exists()
                        || dir.join("stderr.txt").exists()
                        || dir.join("exit_code.txt").exists();
                    if has_expect {
                        dirs.push(dir.to_path_buf());
                    }
                }
            }
        }
        dirs
    }

    fn read_opt(path: &Path) -> Option<String> {
        fs::read_to_string(path)
            .ok()
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
    }

    fn read_exit(path: &Path) -> Option<i32> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
    }

    fn evaluate_golden(expect: &GoldenExpect, output: &std::process::Output) -> UnitStatus {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        let mut mismatches: Vec<String> = Vec::new();
        if let Some(exp) = &expect.stdout {
            let got = stdout.trim_end_matches(['\r', '\n']);
            if got != exp {
                mismatches.push(format!(
                    "stdout mismatch:\n    expected: {:?}\n    got:      {:?}",
                    exp, got
                ));
            }
        }
        if let Some(exp) = &expect.stderr {
            let got = stderr.trim_end_matches(['\r', '\n']);
            if got != exp {
                mismatches.push(format!(
                    "stderr mismatch:\n    expected: {:?}\n    got:      {:?}",
                    exp, got
                ));
            }
        }
        if let Some(exp) = expect.exit
            && code != exp
        {
            mismatches.push(format!("exit code mismatch: expected {} got {}", exp, code));
        }
        if mismatches.is_empty() {
            UnitStatus::Pass
        } else {
            UnitStatus::Fail(mismatches.join("\n"))
        }
    }

    // --- unit model ------------------------------------------------
    struct GoldenExpect {
        stdout: Option<String>,
        stderr: Option<String>,
        exit: Option<i32>,
    }
    struct Unit {
        category: String,
        display: String,
        target_file: PathBuf,
        binary_path: PathBuf,
        skip_run: bool,
        golden: Option<GoldenExpect>,
    }

    // Generated harness sources are build inputs, not cache entries. Keeping
    // them outside `.cache` prevents incremental-cache cleanup from removing
    // a source while the test worker is compiling it.
    let test_dir = cwd.join("bin/debug/test/generated");
    let _ = fs::create_dir_all(&test_dir);
    let test_bin_dir = cwd.join("bin/debug/test");
    if test_bin_dir.exists() && !test_bin_dir.is_dir() {
        let _ = fs::remove_file(&test_bin_dir);
    }
    let _ = fs::create_dir_all(&test_bin_dir);

    fn is_generic_key(k: &str) -> bool {
        matches!(k, "tests" | "dirs" | "paths")
    }

    let mut units: Vec<Unit> = Vec::new();

    let test_roots: Vec<(String, PathBuf)> = if !paths.is_empty() {
        paths
            .iter()
            .map(|p| ("path".to_string(), cwd.join(p)))
            .collect()
    } else {
        read_owl_test_paths(cwd)
    };

    let configured_test_dirs: Vec<PathBuf> = read_owl_test_paths(cwd)
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    let named_directly: Vec<PathBuf> = paths.iter().map(|p| cwd.join(p)).collect();

    for (key, root) in &test_roots {
        let use_key_cat = !is_generic_key(key);
        if root.is_file() {
            let display = root.strip_prefix(cwd).unwrap_or(root).display().to_string();
            let source = fs::read_to_string(root).unwrap_or_default();
            let has_main = source.contains("pub fn main");
            let has_test_fn = source.contains("@[test]");
            let relative = root.strip_prefix(cwd).unwrap_or(root);
            let safe_stem = relative.to_string_lossy().replace(['/', '\\'], "_");
            let binary_path = test_bin_dir.join(&safe_stem);
            units.push(Unit {
                category: if use_key_cat {
                    key.clone()
                } else {
                    String::new()
                },
                display,
                target_file: root.clone(),
                binary_path,
                skip_run: should_skip_run(
                    root,
                    has_main,
                    has_test_fn,
                    &configured_test_dirs,
                    &named_directly,
                ),
                golden: None,
            });
            continue;
        }
        if !root.is_dir() {
            if paths.is_empty() {
                continue;
            }
            eprintln!("warning: test path not found: {}", root.display());
            continue;
        }
        let golden_dirs = find_golden_dirs(root);
        let mut golden_programs: HashSet<PathBuf> = HashSet::new();
        for gd in &golden_dirs {
            golden_programs.insert(gd.join("program.mire"));
            golden_programs.insert(gd.join("program.mr"));
        }
        let mut files = walkdir(root, "*.mire|*.mr")?;
        files.retain(|path| !is_build_artifact(path) && !is_log_web_source(path));
        files.sort();
        for file in files {
            if golden_programs.contains(&file) {
                continue;
            }
            let display = file
                .strip_prefix(cwd)
                .unwrap_or(&file)
                .display()
                .to_string();
            let source = fs::read_to_string(&file).unwrap_or_default();
            let has_main = source.contains("pub fn main");
            let has_load = source.contains("load ");
            let has_test_fn = source.contains("@[test]");
            let relative = file.strip_prefix(cwd).unwrap_or(&file);
            let safe_stem = relative.to_string_lossy().replace(['/', '\\'], "_");
            let (target_file, _stem) = if !has_main {
                let test_path = test_dir.join(format!("{}.mire", safe_stem));
                if has_load || has_test_fn {
                    let _ = fs::write(&test_path, &source);
                } else {
                    let patched = format!("pub fn main: () {{\n{}\n}}\n", source);
                    let _ = fs::write(&test_path, &patched);
                }
                (test_path, safe_stem.clone())
            } else {
                (file.clone(), safe_stem.clone())
            };
            let binary_path = test_bin_dir.join(&safe_stem);
            let category = if use_key_cat {
                key.clone()
            } else {
                unit_category(root, &file)
            };
            units.push(Unit {
                category,
                display,
                target_file,
                binary_path,
                skip_run: should_skip_run(
                    &file,
                    has_main,
                    has_test_fn,
                    &configured_test_dirs,
                    &named_directly,
                ),
                golden: None,
            });
        }
        for gd in golden_dirs {
            let display = gd.strip_prefix(cwd).unwrap_or(&gd).display().to_string();
            let safe_stem = gd
                .strip_prefix(cwd)
                .unwrap_or(&gd)
                .to_string_lossy()
                .replace(['/', '\\'], "_");
            let binary_path = test_bin_dir.join(format!("golden_{}", safe_stem));
            let expect = GoldenExpect {
                stdout: read_opt(&gd.join("stdout.txt")),
                stderr: read_opt(&gd.join("stderr.txt")),
                exit: read_exit(&gd.join("exit_code.txt")),
            };
            let category = if use_key_cat {
                key.clone()
            } else {
                unit_category(root, &gd.join("program.mire"))
            };
            units.push(Unit {
                category,
                display,
                target_file: gd.join("program.mire"),
                binary_path,
                skip_run: false,
                golden: Some(expect),
            });
        }
    }

    // backward-compat: run the project's own entry as a smoke test
    if paths.is_empty() {
        for candidate in [
            cwd.join("code/main.mire"),
            cwd.join("code/main.mr"),
            cwd.join("main.mire"),
            cwd.join("main.mr"),
        ] {
            if candidate.exists() {
                let display = candidate
                    .strip_prefix(cwd)
                    .unwrap_or(&candidate)
                    .display()
                    .to_string();
                let source = fs::read_to_string(&candidate).unwrap_or_default();
                let has_main = source.contains("pub fn main");
                let has_test_fn = source.contains("@[test]");
                let relative = candidate.strip_prefix(cwd).unwrap_or(&candidate);
                let safe_stem = relative.to_string_lossy().replace(['/', '\\'], "_");
                let binary_path = test_bin_dir.join(&safe_stem);
                units.push(Unit {
                    category: String::new(),
                    display,
                    target_file: candidate.clone(),
                    binary_path,
                    skip_run: should_skip_run(
                        &candidate,
                        has_main,
                        has_test_fn,
                        &configured_test_dirs,
                        &named_directly,
                    ),
                    golden: None,
                });
                break;
            }
        }
    }

    if units.is_empty() {
        println!("no tests found");
        return Ok(0);
    }

    let started = Instant::now();
    let jobs = if jobs == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(1)
    } else {
        jobs.max(1)
    };

    let mut results: Vec<(String, String, UnitStatus)> = Vec::new();
    let mut execution_metrics: std::collections::HashMap<String, (u64, u128, f64)> =
        std::collections::HashMap::new();
    let mut warning_codes_by_family: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut warning_counts_by_family: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut all_warnings: Vec<Diagnostic> = Vec::new();

    // Diagnostics are collected for per-family info.json even when the user
    // does not request console output. `show_warn` controls presentation only.
    let warn_filter = WarningFilter::All;

    for chunk in units.chunks(jobs) {
        let compile_results: Vec<Option<Result<mire::BuildResult, MireError>>> =
            std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(chunk.len());
                for u in chunk {
                    let options = BuildOptions {
                        mode: BuildMode::Debug,
                        opt_level,
                        output: Some(u.binary_path.clone()),
                        emit_binary: run,
                        persist_ir: false,
                        import_mode: ImportMode::default(),
                        cache: Default::default(),
                        warning_filter: warn_filter.clone(),
                        deny_warnings: HashSet::new(),
                        test_mode: true,
                        module_paths: Vec::new(),
                        c_defs: c_defs.clone(),
                        ..Default::default()
                    };
                    handles
                        .push(s.spawn(move || compile_file_with_avenys(&u.target_file, &options)));
                }
                handles
                    .into_iter()
                    .map(|h| Some(h.join().unwrap()))
                    .collect()
            });

        for (u, result) in chunk.iter().zip(compile_results.iter()) {
            match result {
                Some(Ok(build)) => {
                    let filtered: Vec<_> = build
                        .warnings_raw
                        .iter()
                        .filter(|d| !should_suppress(d.code.name(), &no_warn_cats))
                        .cloned()
                        .collect();
                    for diagnostic in &filtered {
                        warning_codes_by_family
                            .entry(u.category.clone())
                            .or_default()
                            .push(diagnostic.code.as_str().to_string());
                    }
                    if !filtered.is_empty() {
                        *warning_counts_by_family
                            .entry(u.category.clone())
                            .or_default() += filtered.len() as u32;
                    }
                    if show_warn && position {
                        for d in &filtered {
                            print_warning_detailed(d, true);
                        }
                    } else if show_warn {
                        all_warnings.extend(filtered);
                    }
                    if let Some(expect) = &u.golden {
                        if run {
                            match run_binary_with_metrics(&build.binary_path) {
                                Ok((output, ram_mb, time_ms, cpu_percent)) => {
                                    execution_metrics
                                        .insert(u.display.clone(), (ram_mb, time_ms, cpu_percent));
                                    let status = evaluate_golden(expect, &output);
                                    results.push((u.category.clone(), u.display.clone(), status));
                                }
                                Err(e) => results.push((
                                    u.category.clone(),
                                    u.display.clone(),
                                    UnitStatus::Fail(format!("run error: {}", e)),
                                )),
                            }
                        } else {
                            results.push((
                                u.category.clone(),
                                u.display.clone(),
                                UnitStatus::Compiled,
                            ));
                        }
                    } else if run && !u.skip_run {
                        match run_binary_with_metrics(&build.binary_path) {
                            Ok((output, ram_mb, time_ms, cpu_percent)) => {
                                execution_metrics
                                    .insert(u.display.clone(), (ram_mb, time_ms, cpu_percent));
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                let mut file_failed = 0u32;
                                for line in stdout.lines() {
                                    let trimmed = line.trim();
                                    if trimmed.starts_with("[FAIL]") {
                                        if verbose {
                                            println!("  {}", trimmed);
                                        }
                                        file_failed += 1;
                                    } else if verbose {
                                        println!("  {}", trimmed);
                                    }
                                }
                                let status = if file_failed > 0 {
                                    UnitStatus::Fail(format!("{} assertion(s) failed", file_failed))
                                } else if !output.status.success() {
                                    // A script-style test signals failure by
                                    // exiting non-zero, and not every helper
                                    // prints the `[FAIL]` marker the scanner
                                    // above looks for. Trusting the exit code
                                    // too means a failure cannot be reported as
                                    // a pass merely because a helper chose its
                                    // own wording for the message.
                                    UnitStatus::Fail(format!(
                                        "test binary {}",
                                        describe_status(&output.status)
                                    ))
                                } else {
                                    UnitStatus::Pass
                                };
                                results.push((u.category.clone(), u.display.clone(), status));
                            }
                            Err(e) => results.push((
                                u.category.clone(),
                                u.display.clone(),
                                UnitStatus::Fail(format!("run error: {}", e)),
                            )),
                        }
                    } else {
                        results.push((u.category.clone(), u.display.clone(), UnitStatus::Compiled));
                    }
                }
                Some(Err(e)) => {
                    add_codes_from_text(
                        &mut warning_codes_by_family,
                        &u.category,
                        &format!("{}", e),
                    );
                    results.push((
                        u.category.clone(),
                        u.display.clone(),
                        UnitStatus::Fail(format!("{}", e)),
                    ));
                }
                None => {
                    add_codes_from_text(&mut warning_codes_by_family, &u.category, "unknown error");
                    results.push((
                        u.category.clone(),
                        u.display.clone(),
                        UnitStatus::Fail("unknown error".to_string()),
                    ));
                }
            }
        }
    }

    // --- grouped, categorized output ------------------------------
    let mut categories: Vec<String> = results.iter().map(|(c, _, _)| c.clone()).collect();
    categories.sort();
    categories.dedup();

    let mut global_failed = 0u32;
    let global_skipped = 0u32;

    println!();
    for cat in &categories {
        if categorize && !cat.is_empty() {
            println!("[{}]", cat);
        }
        for (c, display, status) in &results {
            if c != cat {
                continue;
            }
            let indented = categorize && !cat.is_empty();
            match status {
                UnitStatus::Pass => {
                    if indented {
                        println!("  {} ... ok", display);
                    } else {
                        println!("test {} ... ok", display);
                    }
                }
                UnitStatus::Compiled => {
                    if indented {
                        println!("  {} ... ok (compiled)", display);
                    } else {
                        println!("test {} ... ok (compiled)", display);
                    }
                }
                UnitStatus::Fail(detail) => {
                    global_failed += 1;
                    if indented {
                        println!("  {} ... FAILED", display);
                    } else {
                        println!("test {} ... FAILED", display);
                    }
                    for line in detail.lines() {
                        println!("      {}", line);
                    }
                }
            }
        }
    }

    if show_warn && !position && !all_warnings.is_empty() {
        println!();
        print_warning_summary(&all_warnings);
    }

    // A source file is only a compilation unit; the user-facing count must
    // represent the actual @[test] declarations executed inside that unit.
    // This keeps the modular logs useful when one file contains many cases.
    let declared_passed: u32 = results
        .iter()
        .filter(|(_, _, status)| matches!(status, UnitStatus::Pass | UnitStatus::Compiled))
        .map(|(_, display, _)| declared_test_count(cwd, display))
        .sum();
    // A failing unit counts as at least one failure. A file that declares no
    // @[test] function has nothing to count, so summing raw declaration counts
    // reported "Failed: 0" next to a file that had just printed FAILED: a
    // script-style test (a plain main asserting through a helper, as owl's own
    // suite does) could fail as loudly as it liked without ever moving the
    // total. The failure has to be visible where the user is looking.
    let declared_failed: u32 = results
        .iter()
        .filter(|(_, _, status)| matches!(status, UnitStatus::Fail(_)))
        .map(|(_, display, _)| failed_test_count(cwd, display))
        .sum();
    let total = declared_passed + declared_failed + global_skipped;
    println!();
    println!("test result:");
    println!(
        "Ok: {} - Passed: {} - Failed: {} - Filtered Out: {}",
        declared_passed, declared_passed, declared_failed, global_skipped
    );
    println!("Total: {}", total);
    if write_logs {
        write_test_logs(
            cwd,
            &results,
            &execution_metrics,
            &warning_codes_by_family,
            &warning_counts_by_family,
            declared_passed,
            declared_failed,
            global_skipped,
            started.elapsed(),
        );
    }
    let exit_code = if global_failed == 0 { 0 } else { 1 };

    Ok(exit_code)
}

/// Writes reproducible per-family and global metrics for the modular Mire
/// suite. The files are deliberately overwritten on every invocation so a log
/// never combines results from different compiler revisions or test runs.
type FamilyMetrics = (u32, u32, u32, Vec<String>, u64, u128, f64);
#[allow(clippy::too_many_arguments)]
fn write_test_logs(
    cwd: &Path,
    results: &[(String, String, UnitStatus)],
    execution_metrics: &std::collections::HashMap<String, (u64, u128, f64)>,
    warning_codes_by_family: &std::collections::HashMap<String, Vec<String>>,
    warning_counts_by_family: &std::collections::HashMap<String, u32>,
    passed: u32,
    failed: u32,
    filtered: u32,
    elapsed: std::time::Duration,
) {
    let log_root = cwd.join("tests/log");
    if fs::create_dir_all(&log_root).is_err() {
        return;
    }
    // Logs are generated artifacts. Remove only their previous contents so a
    // renamed or deleted family cannot survive into a later report.
    if let Ok(entries) = fs::read_dir(&log_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some("web") {
                    continue;
                }
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }

    let compiler_memory_mb = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .map(|kb| kb.div_ceil(1024))
        .unwrap_or(0);

    let global_memory_mb = results
        .iter()
        .filter_map(|(_, display, _)| execution_metrics.get(display).map(|(ram, _, _)| *ram))
        .max()
        .unwrap_or(compiler_memory_mb);
    let make_count = |ok: u32, err: u32, filt: u32, time_ms: u128, ram_mb: u64| {
        serde_json::json!({
            "Ok": ok,
            "Err": err,
            "FiltOut": filt,
            "time_ms": time_ms,
            "ram_mb": ram_mb
        })
    };

    let global = make_count(
        passed,
        failed,
        filtered,
        elapsed.as_millis(),
        global_memory_mb,
    );

    let mut families: std::collections::BTreeMap<
        String,
        FamilyMetrics,
    > = std::collections::BTreeMap::new();
    for (family, display, status) in results {
        let key = if family.is_empty() {
            "global".to_string()
        } else {
            family_log_name(family)
        };
        let entry = families
            .entry(key)
            .or_insert((0, 0, 0, Vec::new(), 0, 0, 0.0));
        let test_count = declared_test_count(cwd, display);
        match status {
            UnitStatus::Pass | UnitStatus::Compiled => entry.0 += test_count,
            // Same floor as the summary: a failing unit is never worth zero.
            UnitStatus::Fail(_) => entry.1 += failed_test_count(cwd, display),
        }
        if let Some((ram_mb, time_ms, cpu_percent)) = execution_metrics.get(display) {
            entry.4 = entry.4.max(*ram_mb);
            entry.5 = entry.5.max(*time_ms);
            entry.6 = entry.6.max(*cpu_percent);
        }
        entry.3.push(display.clone());
    }

    for (family, (ok, err, filt, files, ram_mb, time_ms, cpu_percent)) in families {
        let count = make_count(ok, err, filt, time_ms, ram_mb);
        let family_dir = log_root.join(&family);
        let _ = fs::create_dir_all(&family_dir);
        let count_raw = serde_json::to_string_pretty(&count).unwrap_or_else(|_| "{}".to_string());
        let _ = fs::write(family_dir.join("count.json"), format!("{}\n", count_raw));
        let mut codes: Vec<String> = warning_codes_by_family
            .iter()
            .filter(|(raw_family, _)| family_log_name(raw_family) == family)
            .flat_map(|(_, family_codes)| family_codes.iter().cloned())
            .collect();
        codes.sort();
        codes.dedup();
        let warning_count: u32 = warning_counts_by_family
            .iter()
            .filter(|(raw_family, _)| family_log_name(raw_family) == family)
            .map(|(_, count)| *count)
            .sum();
        let info = serde_json::json!({
            "RAM": ram_mb,
            "CPU %": (cpu_percent * 100.0).round() / 100.0,
            "Time ms": time_ms,
            "Total": ok + err + filt,
            "Warns": warning_count,
            "CODES": codes
        });
        let info_raw = serde_json::to_string_pretty(&info).unwrap_or_else(|_| "{}".to_string());
        let _ = fs::write(family_dir.join("info.json"), format!("{}\n", info_raw));
        let details = serde_json::json!({
            "family": family,
            "files": files,
            "count": count,
            "info": info,
            "reproducible": true
        });
        let details_raw =
            serde_json::to_string_pretty(&details).unwrap_or_else(|_| "{}".to_string());
        let _ = fs::write(family_dir.join("log.json"), format!("{}\n", details_raw));
    }

    let global_dir = log_root.join("global");
    let _ = fs::create_dir_all(&global_dir);
    let global_raw = serde_json::to_string_pretty(&global).unwrap_or_else(|_| "{}".to_string());
    let _ = fs::write(global_dir.join("count.json"), format!("{}\n", global_raw));
    let _ = fs::write(
        global_dir.join("log.json"),
        format!(
            "{{\"scope\":\"global\",\"count\":{},\"families\":{}}}\n",
            global_raw,
            results.len()
        ),
    );
}

/// Renders a child exit status for a failure message, naming the two ways a
/// test process can end badly without a `[FAIL]` line: a plain non-zero exit
/// (an assertion helper calling `exit(1)`) and an abnormal death (a signal, or
/// a panic that aborts). The second is worth distinguishing, because
/// "exited with signal 6 (Aborted)" points at a crash rather than an
/// assertion.
fn describe_status(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.code() {
        Some(code) => format!("exited with code {code}"),
        None => match status.signal() {
            Some(sig) => format!("was killed by signal {sig}"),
            None => "ended without an exit code".to_string(),
        },
    }
}

/// Runs one compiled test in its own process and records its real peak RSS.
/// `/proc/<pid>/status` is sampled while it runs; the maximum observed RSS is
/// retained, so the metric is a peak for that test process rather than a mean.
fn run_binary_with_metrics(path: &Path) -> std::io::Result<(std::process::Output, u64, u128, f64)> {
    let started = Instant::now();
    let mut child = Command::new(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let child_pid = child.id();
    let mut peak_rss_kb = 0u64;
    let mut peak_cpu_runtime_ns = 0u64;
    loop {
        if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", child_pid))
            && let Some(rss) = status.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            }) {
                peak_rss_kb = peak_rss_kb.max(rss);
            }
        peak_cpu_runtime_ns = peak_cpu_runtime_ns.max(proc_cpu_runtime_ns(child_pid));
        if child.try_wait()?.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let output = child.wait_with_output()?;
    let elapsed = started.elapsed().as_millis();
    peak_cpu_runtime_ns = peak_cpu_runtime_ns.max(proc_cpu_runtime_ns(child_pid));
    let elapsed_ns = started.elapsed().as_nanos().max(1) as f64;
    let cpu_percent = (peak_cpu_runtime_ns as f64 / elapsed_ns) * 100.0;
    Ok((output, peak_rss_kb.div_ceil(1024), elapsed, cpu_percent))
}

/// Reads actual CPU execution time from procfs. `schedstat` has finer
/// resolution than jiffies, so short independent tests are not rounded to
/// zero merely because they finish within one scheduler tick.
fn proc_cpu_runtime_ns(pid: u32) -> u64 {
    let Ok(stat) = fs::read_to_string(format!("/proc/{}/schedstat", pid)) else {
        return 0;
    };
    stat.split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Counts test declarations in the original source file so metrics report
/// logical cases instead of merely counting the generated compilation units.
fn declared_test_count(cwd: &Path, display: &str) -> u32 {
    let path = cwd.join(display);
    fs::read_to_string(path)
        .ok()
        .map(|source| source.matches("@[test]").count() as u32)
        .unwrap_or(0)
}

/// How much a failing unit contributes to the totals.
///
/// Counting raw `@[test]` declarations is right for a file that declares them,
/// and wrong for one that does not. A script-style test — a plain `main` that
/// asserts through a helper, the shape owl's own suite uses — declares nothing,
/// so a failure inside it summed to zero and the summary printed `Failed: 0`
/// directly beneath a `FAILED` line. The exit code was never wrong, which is
/// exactly why this went unnoticed: only the number a human reads was lying.
/// Decides whether a collected test unit should be compiled but not executed.
///
/// `main` without `@[test]` normally means "a program that got swept into a
/// broad directory argument, not a test", and running it could have side
/// effects. But a script-style test *is* a `main` that asserts through a helper
/// and declares nothing, and a whole suite can be written that way — owl's is,
/// file after file. Skipping those reported every one of them `ok (compiled)`
/// having run no assertion at all, which is the same vacuous green that a
/// shared test artifact produced, one layer further out.
///
/// So the skip is narrowed to what it was actually for. A file inside a test
/// location the manifest configures is a test by construction and always runs;
/// so does a file the user named outright. Only a file reached by walking a
/// directory that is not a configured test location is left alone.
fn should_skip_run(
    file: &Path,
    has_main: bool,
    has_test_fn: bool,
    configured_test_dirs: &[PathBuf],
    named_directly: &[PathBuf],
) -> bool {
    has_main
        && !has_test_fn
        && !configured_test_dirs.iter().any(|dir| file.starts_with(dir))
        && !named_directly.iter().any(|named| named == file)
}

fn failed_test_count(cwd: &Path, display: &str) -> u32 {
    declared_test_count(cwd, display).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_unit(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).expect("write unit");
    }

    #[test]
    fn a_failing_unit_without_test_declarations_still_counts_as_one() {
        let dir = std::env::temp_dir().join(format!(
            "mire_failed_count_{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        // The shape owl's suite uses: a plain main asserting through a helper,
        // with no @[test] declarations anywhere.
        write_unit(
            &dir,
            "script.mr",
            "pub fn main: () { if 1 != 2 { exit(1) } }",
        );
        // A conventional unit declaring three cases, one of which failed.
        write_unit(
            &dir,
            "declared.mr",
            "@[test] pub fn a: () :bool { return true }\n\
             @[test] pub fn b: () :bool { return false }\n\
             @[test] pub fn c: () :bool { return true }",
        );

        // Zero declarations, but a failure is still one failure.
        assert_eq!(declared_test_count(&dir, "script.mr"), 0);
        assert_eq!(failed_test_count(&dir, "script.mr"), 1);
        // Declared units keep their real count rather than being floored to one.
        assert_eq!(declared_test_count(&dir, "declared.mr"), 3);
        assert_eq!(failed_test_count(&dir, "declared.mr"), 3);
        // A unit that cannot even be read must not vanish either.
        assert_eq!(failed_test_count(&dir, "missing.mr"), 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn script_style_units_run_unless_they_are_a_swept_up_program() {
        let tests_dir = Path::new("/proj/tests");
        let configured = vec![tests_dir.to_path_buf()];

        // The case that was broken: a `main` asserting through a helper, with
        // no @[test] anywhere, living in a configured test directory. This is
        // owl's entire suite. It must execute, or the suite is vacuously green.
        assert!(!should_skip_run(
            &tests_dir.join("06_math.mr"),
            true,
            false,
            &configured,
            &[]
        ));

        // A conventional unit declares its cases and always runs.
        assert!(!should_skip_run(
            &tests_dir.join("ok.mr"),
            false,
            true,
            &configured,
            &[]
        ));

        // What the skip is actually for: `code/main.mr` has a main, declares
        // nothing, and was reached by walking the project directory. Running it
        // would execute a program for its side effects, so it stays skipped.
        assert!(should_skip_run(
            Path::new("/proj/code/main.mr"),
            true,
            false,
            &configured,
            &[]
        ));

        // Naming the file outright is a request to test it, so it runs even
        // though it looks exactly like a swept-up program.
        assert!(!should_skip_run(
            Path::new("/proj/code/main.mr"),
            true,
            false,
            &configured,
            &[PathBuf::from("/proj/code/main.mr")]
        ));

        // A bare main outside the test tree, with nothing configured and
        // nothing named, keeps the original conservative behaviour.
        assert!(should_skip_run(
            Path::new("/proj/main.mr"),
            true,
            false,
            &[],
            &[]
        ));
    }

    #[test]
    fn a_non_zero_exit_fails_a_unit_even_without_a_fail_marker() {
        // A helper that exits non-zero without printing `[FAIL]` must not be
        // reported as a pass. The exit status is the last line of defence: it
        // holds even if the wording of a failure message changes.
        let failed = Command::new("/bin/sh").arg("-c").arg("exit 3").status().unwrap();
        assert!(!failed.success());
        assert_eq!(describe_status(&failed), "exited with code 3");

        let ok = Command::new("/bin/sh").arg("-c").arg("exit 0").status().unwrap();
        assert!(ok.success());
    }
}

fn family_log_name(name: &str) -> String {
    if name.eq_ignore_ascii_case("POO") {
        return "poo".to_string();
    }
    if name.eq_ignore_ascii_case("FFI") {
        return "ffi".to_string();
    }
    let mut result = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            result.push('_');
        }
        result.push(character.to_ascii_lowercase());
    }
    result
}

/// Extracts stable diagnostic identifiers from compiler errors so a family
/// report remains useful even when compilation stops before a BuildResult is
/// available.
fn add_codes_from_text(
    codes_by_family: &mut std::collections::HashMap<String, Vec<String>>,
    family: &str,
    text: &str,
) {
    let mut codes = Vec::new();
    for token in text.split(|character: char| !character.is_ascii_alphanumeric()) {
        if (token.starts_with('E') || token.starts_with('W'))
            && token.len() == 5
            && token[1..]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            codes.push(token.to_string());
        }
    }
    if !codes.is_empty() {
        codes_by_family
            .entry(family.to_string())
            .or_default()
            .extend(codes);
    }
}

fn is_build_artifact(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(name)
                if matches!(name.to_str(), Some(".git" | ".cache" | "target" | "bin"))
        )
    })
}

/// Keeps the dashboard source under tests/log without treating it as a test
/// unit or deleting it during generated-log rotation.
fn is_log_web_source(path: &Path) -> bool {
    let mut saw_tests = false;
    let mut saw_log = false;
    for component in path.components() {
        let Some(name) = component.as_os_str().to_str() else {
            continue;
        };
        if name == "tests" {
            saw_tests = true;
        }
        if saw_tests && name == "log" {
            saw_log = true;
        }
        if saw_log && name == "web" {
            return true;
        }
    }
    false
}

/// Returns true if `path` is underneath any of `test_roots`.
pub(crate) fn is_under_test_path(path: &Path, test_roots: &[PathBuf]) -> bool {
    test_roots.iter().any(|root| path.starts_with(root))
}

/// Read owl.toml [tests] keys to discover test root directories.
pub(crate) fn read_test_roots(cwd: &Path) -> Vec<PathBuf> {
    let manifest_paths = [
        cwd.join("owl.toml"),
        cwd.join("Mire.toml"),
        cwd.join("Avenys.toml"),
    ];
    let mut content = String::new();
    for m in &manifest_paths {
        if let Ok(c) = fs::read_to_string(m) {
            content = c;
            break;
        }
    }
    if content.is_empty() {
        return Vec::new();
    }
    let mut in_section = String::new();
    let mut found: Vec<String> = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line[1..line.len() - 1].to_string();
            continue;
        }
        if in_section == "tests" {
            if let Some(v) = kv_string(line, "path") {
                found.push(v);
            } else if let Some(v) = kv_string(line, "dirs") {
                for p in parse_array_value(&v) {
                    if !p.is_empty() {
                        found.push(p);
                    }
                }
            } else if let Some((_key, val)) = parse_generic_kv(line) {
                found.push(val);
            }
        } else if in_section == "paths"
            && let Some(v) = kv_string(line, "tests") {
                found.push(v);
            }
    }
    if found.is_empty() {
        found.push("tests".to_string());
    }
    found.into_iter().map(|p| cwd.join(p)).collect()
}

pub(crate) fn parse_array_value(s: &str) -> Vec<String> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|part| part.trim().trim_matches('"').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

pub(crate) fn parse_generic_kv(line: &str) -> Option<(String, String)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim().to_string();
    if key.starts_with('[') || key.is_empty() {
        return None;
    }
    let val = line[eq_pos + 1..].trim();
    let val = val.trim_matches('"').to_string();
    Some((key, val))
}

pub(crate) fn kv_string(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{}=", key);
    let rest = line.strip_prefix(&prefix)?;
    let rest = rest.trim();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    if rest.starts_with('[') {
        let inner = rest.trim_start_matches('[').trim_end_matches(']');
        for part in inner.split(',') {
            let p = part.trim().trim_matches('"').to_string();
            if !p.is_empty() {
                return Some(p);
            }
        }
    }
    None
}
