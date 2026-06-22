use mire::{
    BuildMode, BuildOptions, OptLevel, compile_file_with_avenys,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nonce}"))
}

fn make_project_dir(prefix: &str) -> PathBuf {
    let root = unique_temp_dir(prefix);
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn read_vmpeak() -> u64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if line.starts_with("VmPeak:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse::<u64>().unwrap_or(0);
            }
        }
    }
    0
}

fn read_vmrss() -> u64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse::<u64>().unwrap_or(0);
            }
        }
    }
    0
}

fn bench_compile(
    name: &str,
    source: &str,
    opt_level: OptLevel,
) {
    let root = make_project_dir(&format!("bench_{name}"));
    let source_path = root.join("main.mire");
    fs::write(
        root.join("owl.toml"),
        "[project]\nname = \"bench\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
    )
    .expect("write owl.toml");
    fs::write(&source_path, source).expect("write source");

    // Measure pre-compilation memory
    let pre_rss = read_vmrss();
    let pre_peak = read_vmpeak();

    // Time compilation
    let start = Instant::now();
    let build = compile_file_with_avenys(
        &source_path,
        &BuildOptions {
            mode: BuildMode::Debug,
            opt_level,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        },
    )
    .expect("compile");
    let compile_time = start.elapsed();

    let post_rss = read_vmrss();
    let post_peak = read_vmpeak();

    // Binary size
    let bin_size = fs::metadata(&build.binary_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Time execution
    let run_start = Instant::now();
    let output = Command::new(&build.binary_path)
        .output()
        .expect("run binary");
    let run_time = run_start.elapsed();

    let rss_delta = if post_rss > pre_rss {
        post_rss - pre_rss
    } else {
        0
    };
    let peak_delta = if post_peak > pre_peak {
        post_peak - pre_peak
    } else {
        0
    };

    println!(
        "[BENCH] {name:<35} | opt={opt:?} | compile={ct:.3}s | rss={rss}KB | peak={peak}KB | bin={bin}B | run={rt:.3}s | {status}",
        name = name,
        opt = opt_level,
        ct = compile_time.as_secs_f64(),
        rss = rss_delta,
        peak = peak_delta,
        bin = bin_size,
        rt = run_time.as_secs_f64(),
        status = if output.status.success() { "OK" } else { "FAIL" },
    );
}

#[test]
fn benchmark_smoke() {
    println!();
    println!("══════════════════════════════════════════════════════════════");
    println!("  Mire Compiler Benchmarks");
    println!("══════════════════════════════════════════════════════════════");
    println!();

    // 1) Simple program (hello world)
    bench_compile(
        "hello_world",
        "pub fn main: () {\n    use dasu(\"hello\")\n}\n",
        OptLevel::O0,
    );
    bench_compile(
        "hello_world",
        "pub fn main: () {\n    use dasu(\"hello\")\n}\n",
        OptLevel::O3,
    );

    // 2) Fibonacci recursive (moderate complexity)
    bench_compile(
        "fibonacci(20)",
        "fn fib: (n: i64) :i64 {\n  if n <= 1 { return n }\n  return fib(n - 1) + fib(n - 2)\n}\npub fn main: () {\n  set r = fib(20)\n  use dasu(str(r))\n}\n",
        OptLevel::O0,
    );
    bench_compile(
        "fibonacci(20)",
        "fn fib: (n: i64) :i64 {\n  if n <= 1 { return n }\n  return fib(n - 1) + fib(n - 2)\n}\npub fn main: () {\n  set r = fib(20)\n  use dasu(str(r))\n}\n",
        OptLevel::O3,
    );

    // 3) Mixed workload (recursion + loops + arithmetic)
    bench_compile(
        "mixed_workload",
        "fn calc: (n: i64) :i64 {\n  if n <= 0 { return 0 }\n  set sum = 0 :i64 mut\n  set i = 0 :i64 mut\n  while i < 100 {\n    set sum = sum + i\n    set i = i + 1\n  }\n  return sum + calc(n - 1)\n}\npub fn main: () {\n  set r = calc(100)\n  use dasu(str(r))\n}\n",
        OptLevel::O0,
    );
    bench_compile(
        "mixed_workload",
        "fn calc: (n: i64) :i64 {\n  if n <= 0 { return 0 }\n  set sum = 0 :i64 mut\n  set i = 0 :i64 mut\n  while i < 100 {\n    set sum = sum + i\n    set i = i + 1\n  }\n  return sum + calc(n - 1)\n}\npub fn main: () {\n  set r = calc(100)\n  use dasu(str(r))\n}\n",
        OptLevel::O3,
    );

    // 4) Struct + impl benchmark (complex types without array return issue)
    bench_compile(
        "struct_matrix",
        "load kioto\nstruct Matrix { data :arr[i64 9] rows :i64 cols :i64 }\nimpl Matrix {\n  fn new: (r :i64, c :i64) :Matrix { return (Matrix data: [0 0 0 0 0 0 0 0 0] :arr[i64 9], rows: r, cols: c) }\n  fn set: (self, r :i64, c :i64, v :i64) { set idx = r * self.cols + c\n    set self.data at idx = v }\n  fn get: (self, r :i64, c :i64) :i64 { set idx = r * self.cols + c\n    return self.data at idx }\n}\npub fn main: () {\n  set m = Matrix::new(3 3)\n  m.set(0 0 1)\n  m.set(1 1 5)\n  m.set(2 2 9)\n  set v = m.get(1 1)\n  use dasu(str(v))\n}\n",
        OptLevel::O0,
    );
    bench_compile(
        "struct_matrix_O3",
        "load kioto\nstruct Matrix { data :arr[i64 9] rows :i64 cols :i64 }\nimpl Matrix {\n  fn new: (r :i64, c :i64) :Matrix { return (Matrix data: [0 0 0 0 0 0 0 0 0] :arr[i64 9], rows: r, cols: c) }\n  fn set: (self, r :i64, c :i64, v :i64) { set idx = r * self.cols + c\n    set self.data at idx = v }\n  fn get: (self, r :i64, c :i64) :i64 { set idx = r * self.cols + c\n    return self.data at idx }\n}\npub fn main: () {\n  set m = Matrix::new(3 3)\n  m.set(0 0 1)\n  m.set(1 1 5)\n  m.set(2 2 9)\n  set v = m.get(1 1)\n  use dasu(str(v))\n}\n",
        OptLevel::O3,
    );

    // 5) HOFs (map/filter/fold with closures)
    bench_compile(
        "list_hofs",
        "load kioto\npub fn main: () {\n  set s = lists.fold(0, (a b) => a + b, [1 2 3 4 5 6 7 8 9 10])\n  set d = lists.map((x) => x * 2, [1 2 3 4 5])\n  set f = lists.filter((x) => x > 3, [1 2 3 4 5 6 7 8 9 10])\n  use dasu(str(s))\n}\n",
        OptLevel::O0,
    );
    bench_compile(
        "list_hofs_O3",
        "load kioto\npub fn main: () {\n  set s = lists.fold(0, (a b) => a + b, [1 2 3 4 5 6 7 8 9 10])\n  set d = lists.map((x) => x * 2, [1 2 3 4 5])\n  set f = lists.filter((x) => x > 3, [1 2 3 4 5 6 7 8 9 10])\n  use dasu(str(s))\n}\n",
        OptLevel::O3,
    );

    // 6) Large iteration (512 iterations)
    bench_compile(
        "loop_512",
        "load kioto\npub fn main: () {\n  set s = 0 :i64 mut\n  set i = 0 :i64 mut\n  while i < 512 {\n    set s = s + i\n    set i = i + 1\n  }\n  use dasu(str(s))\n}\n",
        OptLevel::O3,
    );

    // 7) Big recursion with 512 depth
    bench_compile(
        "deep_recursion_512",
        "fn rc: (n: i64, max: i64) :i64 {\n  if n >= max { return max }\n  return rc(n + 1, max)\n}\npub fn main: () {\n  set r = rc(0, 512)\n  use dasu(str(r))\n}\n",
        OptLevel::O3,
    );

    // 8) String-heavy workload
    bench_compile(
        "string_ops",
        "load kioto\npub fn main: () {\n  set s = \"\" :str mut\n  set i = 0 :i64 mut\n  while i < 100 {\n    set s = s + str(i)\n    set i = i + 1\n  }\n  set l = strings.len(s)\n  use dasu(str(l))\n}\n",
        OptLevel::O3,
    );

    // 9) Cache hit (incremental compilation — second compile of same source)
    {
        let root = make_project_dir("bench_cache_hit");
        let source_path = root.join("main.mire");
        fs::write(
            root.join("owl.toml"),
            "[project]\nname = \"bench\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
        ).expect("write owl.toml");
        fs::write(&source_path, "pub fn main: () {\n    use dasu(\"hello\")\n}\n").expect("write");
        let opts = BuildOptions {
            mode: BuildMode::Debug,
            opt_level: OptLevel::O0,
            debug_dump: false,
            output: None,
            emit_binary: true,
            persist_ir: false,
            import_mode: mire::ImportMode::Reachable,
            cache: Default::default(),
            warning_filter: mire::error::diagnostic::WarningFilter::Default,
            deny_warnings: std::collections::HashSet::new(),
            module_paths: vec![],
        };
        // Cold compile
        let start = Instant::now();
        let _cold = compile_file_with_avenys(&source_path, &opts).expect("cold");
        let cold_time = start.elapsed();
        // Hot compile (cache hit)
        let start = Instant::now();
        let _hot = compile_file_with_avenys(&source_path, &opts).expect("hot");
        let hot_time = start.elapsed();
        println!(
            "[BENCH] cache_hit                            |             | cold={ct:.3}s | hot={ht:.3}s | speedup={sp:.1}x",
            ct = cold_time.as_secs_f64(),
            ht = hot_time.as_secs_f64(),
            sp = cold_time.as_secs_f64() / hot_time.as_secs_f64().max(0.001),
        );
    }

    // 10) Large integer arithmetic
    bench_compile(
        "big_math",
        "pub fn main: () {\n  set r = 0 :i64 mut\n  set i = 0 :i64 mut\n  while i < 10000 {\n    set r = r + i * i\n    set i = i + 1\n  }\n  use dasu(str(r))\n}\n",
        OptLevel::O3,
    );

    println!();
    println!("══════════════════════════════════════════════════════════════");
    println!("  Done.");
    println!("══════════════════════════════════════════════════════════════");
    println!();
}
