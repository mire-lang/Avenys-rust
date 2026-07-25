use crate::cli::*;
use mire::error::diagnostic::{Diagnostic, WarningFilter};
use mire::{BuildMode, BuildOptions, ImportMode, MireError, OptLevel, compile_file_with_avenys};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn test_command(cwd: &Path, args: &[String]) -> Result<i32, MireError> {
    let mut run = true;
    let mut verbose = false;
    let mut jobs: usize = 0;
    let mut owl_home = None;
    let mut paths: Vec<String> = Vec::new();
    let mut opt_level = OptLevel::O0;
    let mut categorize = true;
    let mut show_warn = false;
    let mut position = false;
    let mut no_warn_cats: Vec<String> = Vec::new();

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
                println!("  --owl-home <path>   Override the Owl module cache root");
                println!("  -O, --opt-level <n> Optimization level for test binaries (0,1,2,3,s,z)");
                println!("  -r, --release       Shorthand for --opt-level 3");
                println!("  -d, --debug         Shorthand for --opt-level 0 (default)");
                println!("  --show-warn, --sh-warn  Show warnings (summary by default)");
                println!("  --position, --pos       Show warnings per-file (detailed)");
                println!("  --no-warn <cat>         Suppress warning category (repeatable)");
                println!("  --help, -h          Show this help message");
                return Ok(0);
            }
            "--no-run" => run = false,
            "--verbose" | "-v" => verbose = true,
            "--no-categorize" => categorize = false,
            "--show-warn" | "--sh-warn" => show_warn = true,
            "--position" | "--pos" => position = true,
            "--no-warn" => {
                i += 1;
                let cat = args.get(i).ok_or_else(|| cli_msg("Missing warning category after --no-warn"))?;
                no_warn_cats.push(cat.clone());
            }
            "--jobs" | "-j" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    cli_msg("Missing value for --jobs")
                })?;
                jobs = value.parse().map_err(|_| {
                    cli_msg("--jobs must be a positive integer")
                })?;
            }
            "--owl-home" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| cli_msg("Missing value for --owl-home"))?;
                owl_home = Some(PathBuf::from(value));
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
                    jobs = val.parse().map_err(|_| {
                        cli_msg("--jobs must be a positive integer")
                    })?;
                } else {
                    paths.push(args[i].clone());
                }
            }
        }
        i += 1;
    }

    set_owl_home_env(owl_home.as_ref());

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
            } else if in_section == "paths" {
                if let Some(v) = kv_string(line, "tests") {
                    found.push(("paths".to_string(), v));
                }
            }
        }
        if found.is_empty() {
            found.push(("tests".to_string(), "tests".to_string()));
        }
        found.into_iter().map(|(k, p)| (k, cwd.join(p))).collect()
    }

    fn kv_string(line: &str, key: &str) -> Option<String> {
        let prefix = format!("{}=", key);
        let rest = line.strip_prefix(&prefix)?;
        let rest = rest.trim();
        if rest.starts_with('"') {
            let end = rest[1..].find('"')?;
            return Some(rest[1..1 + end].to_string());
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
                    .map(|n| n == "program.mire")
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
        if let Some(exp) = expect.exit {
            if code != exp {
                mismatches.push(format!("exit code mismatch: expected {} got {}", exp, code));
            }
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
    enum UnitStatus {
        Pass,
        Fail(String),
        Compiled,
    }
    struct Unit {
        category: String,
        display: String,
        target_file: PathBuf,
        binary_path: PathBuf,
        skip_run: bool,
        golden: Option<GoldenExpect>,
    }

    let test_dir = cwd.join("bin/.cache/test");
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
        paths.iter().map(|p| ("path".to_string(), cwd.join(p))).collect()
    } else {
        read_owl_test_paths(cwd)
    };

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
                category: if use_key_cat { key.clone() } else { String::new() },
                display,
                target_file: root.clone(),
                binary_path,
                skip_run: has_main && !has_test_fn,
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
        }
        let mut files = walkdir(root, "*.mire")?;
        files.sort();
        for file in files {
            if golden_programs.contains(&file) {
                continue;
            }
            let display = file.strip_prefix(cwd).unwrap_or(&file).display().to_string();
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
            let category = if use_key_cat { key.clone() } else { unit_category(root, &file) };
            units.push(Unit {
                category,
                display,
                target_file,
                binary_path,
                skip_run: has_main && !has_test_fn,
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
        for candidate in [cwd.join("code/main.mire"), cwd.join("main.mire")] {
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
                    skip_run: has_main && !has_test_fn,
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

    let jobs = if jobs == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(1)
    } else {
        jobs.max(1)
    };

    let mut results: Vec<(String, String, UnitStatus)> = Vec::new();
    let mut all_warnings: Vec<Diagnostic> = Vec::new();

    let warn_filter = if show_warn {
        WarningFilter::All
    } else {
        WarningFilter::Off
    };

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
                        ..Default::default()
                    };
                    handles.push(s.spawn(move || compile_file_with_avenys(&u.target_file, &options)));
                }
                handles.into_iter().map(|h| Some(h.join().unwrap())).collect()
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
                    if show_warn && position {
                        for d in &filtered {
                            print_warning_detailed(d, true);
                        }
                    } else if show_warn {
                        all_warnings.extend(filtered);
                    }
                    if let Some(expect) = &u.golden {
                        if run {
                            match Command::new(&build.binary_path).output() {
                                Ok(output) => {
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
                        match Command::new(&build.binary_path).output() {
                            Ok(output) => {
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
                                let status = if file_failed == 0 {
                                    UnitStatus::Pass
                                } else {
                                    UnitStatus::Fail(format!(
                                        "{} assertion(s) failed",
                                        file_failed
                                    ))
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
                    results.push((
                        u.category.clone(),
                        u.display.clone(),
                        UnitStatus::Fail(format!("{}", e)),
                    ));
                }
                None => {
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

    let mut global_passed = 0u32;
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
                    global_passed += 1;
                    if indented {
                        println!("  {} ... ok", display);
                    } else {
                        println!("test {} ... ok", display);
                    }
                }
                UnitStatus::Compiled => {
                    global_passed += 1;
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

    let total = global_passed + global_failed + global_skipped;
    println!();
    println!("test result:");
    println!(
        "Ok: {} - Passed: {} - Failed: {} - Filtered Out: {}",
        global_passed, global_passed, global_failed, global_skipped
    );
    println!("Total: {}", total);
    let exit_code = if global_failed == 0 { 0 } else { 1 };

    Ok(exit_code)
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
        if in_section == "tests" || in_section == "paths" {
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
    if rest.starts_with('"') {
        let end = rest[1..].find('"')?;
        return Some(rest[1..1 + end].to_string());
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
