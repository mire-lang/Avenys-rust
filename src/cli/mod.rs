pub(crate) mod commands;
pub(crate) mod test;
pub(crate) mod options;
pub(crate) mod warnings;
pub(crate) mod files;

pub(crate) use test::*;
pub(crate) use options::*;
pub(crate) use warnings::*;
pub(crate) use files::*;

use mire::MireError;
use std::env;
use std::process::ExitCode;

pub(crate) fn main() -> ExitCode {
    match run_cli() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("{}", err.format_color());
            ExitCode::from(1)
        }
    }
}

pub(crate) fn run_cli() -> Result<i32, MireError> {
    let args: Vec<String> = env::args().collect();
    let cwd = env::current_dir().map_err(runtime_err)?;

    if args.len() <= 1 {
        print_help();
        return Ok(1);
    }

    match args[1].as_str() {
        "run" => commands::run_command(&cwd, &args[2..]),
        "build" => commands::build_command(&cwd, &args[2..]),
        "check" => commands::check_command(&cwd, &args[2..]),
        "debug" => commands::debug_command(&cwd, &args[2..]),
        "test" => test::test_command(&cwd, &args[2..]),

        "help" | "--help" | "-h" => {
            print_help();
            Ok(0)
        }
        "--version" | "-V" => {
            println!("Mire / Avenys v{}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        _ => {
            print_help();
            Ok(1)
        }
    }
}

pub(crate) fn print_help() {
    println!("Mire / Avenys v{}", env!("CARGO_PKG_VERSION"));
    println!("Usage: mire <run|build|check|debug> [file] [options]\n");
    println!("Mire is the Avenys compiler. For project management, dependencies,");
    println!("and scaffolding, use Owl (owl new / owl run / owl import).\n");
    println!("Profiles:");
    println!("  --debug               Build profile debug (default)");
    println!("  --release             Build profile release");
    println!("  -O, --opt-level <n>   0|1|2|3|s|z");
    println!("  --owl-home <path>     Override the Owl module cache root");
    println!("\nWarnings (for build/check/run):");
    println!("  --show-warn           Show warning summary");
    println!("  --position            Show per-file warning locations");
    println!("  --no-warn <cat>       Suppress warning category (repeatable)");
    println!("  -W <code>             Promote warning to error");
    println!("  --deny <code>         Deny specific warning code");
    println!("\nCommands:");
    println!("  run [file] [-- args]  Compile + execute");
    println!("  build [file]          Compile only");
    println!("  check [file]          Analyze only");
    println!("  debug [file]          Debug build, emits IR");
    println!("  test [paths...]       Run integration tests from tests/");
    println!("    --no-run            Compile only, skip execution");
    println!("    --verbose, -v       Show per-test results");
    println!("    --show-warn         Show warning summary");
    println!("    --position          Show per-file warning locations");
    println!("    --jobs, -j <n>      Parallel compilation jobs (0 = logical CPUs)");
}
