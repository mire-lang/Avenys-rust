pub(crate) mod commands;
pub(crate) mod files;
pub(crate) mod hash;
pub(crate) mod options;
pub(crate) mod test;
pub(crate) mod warnings;

pub(crate) use files::*;
pub(crate) use hash::*;
pub(crate) use options::*;
pub(crate) use test::*;
pub(crate) use warnings::*;

use mire::MireError;
use mire::error::format::{ColorPolicy, ErrorFormat};
use std::env;
use std::process::ExitCode;

pub(crate) fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let (format, color) = output_options(&args);
    match run_cli() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            let rendered = err.format_for_policy(format, color);
            eprint!("{}", rendered);
            if !rendered.ends_with('\n') {
                eprintln!();
            }
            ExitCode::from(1)
        }
    }
}

fn output_options(args: &[String]) -> (ErrorFormat, ColorPolicy) {
    let mut format = ErrorFormat::default();
    let mut color = ColorPolicy::default();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" {
            break;
        }
        let inline = args[i].split_once('=').map(|(_, value)| value);
        if let Some(value) = option_value(args, &mut i, "--error-format", inline) {
            if let Some(parsed) = ErrorFormat::parse(&value) {
                format = parsed;
            }
        } else if let Some(value) = option_value(args, &mut i, "--message-format", inline) {
            format = match value.as_str() {
                "human" => ErrorFormat::Human,
                "json" => ErrorFormat::Json,
                _ => format,
            };
        } else if let Some(value) = option_value(args, &mut i, "--color", inline) {
            if let Some(parsed) = ColorPolicy::parse(&value) {
                color = parsed;
            }
        }
        i += 1;
    }
    (format, color)
}

fn option_value(
    args: &[String],
    i: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Option<String> {
    if let Some(value) = inline {
        return Some(value.to_string());
    }
    if args[*i] == flag {
        let value = args.get(*i + 1).cloned();
        if value.is_some() {
            *i += 1;
        }
        return value;
    }
    None
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
    println!("Usage: mire <run|build|debug|test> [file] [options]\n");
    println!("Mire is the Avenys compiler. For project management, dependencies,");
    println!("and scaffolding, use Owl (owl new / owl run / owl import).\n");
    println!("Profiles:");
    println!("  --debug               Build profile debug (default)");
    println!("  --release             Build profile release");
    println!("  -O, --opt-level <n>   0|1|2|3|s|z");
    println!("  --lib-dir <path>      Extra fallback package directory");
    println!("  --output-dir <path>   Output directory");
    println!("  --cache-dir <path>    Incremental cache directory");
    println!("  --config <file>       Owl-generated normalized compiler config");
    println!("  -L, --link <name=path> Ecosystem library (repeatable)");
    println!("  -H, --hash <sha256:hex> Expected hash for the preceding --link");
    println!("  --link-dir <dir>      Native linker search directory");
    println!("  --bundle <mode>       static|static-strict|dynamic (default: static)");
    println!("  -l, --link-lib <name> Native library to link");
    println!("  --target <triple>     LLVM/Clang target triple");
    println!(
        "  --cpu <name>          LLVM CPU for instruction selection (native, x86-64-v3, ...);"
    );
    println!("  --runtime <tier>      minimal|none");
    println!("  --artifact <type>     bin|static|shared");
    println!("  --error-format <fmt>  human|short|json|json-rendered-ansi|sarif");
    println!("  --color <policy>      auto|always|never");
    println!();
    println!("Native C (all repeatable, so a build orchestrator needs no config file):");
    println!("  --c-source <path>     C source file to compile and link");
    println!("  --c-include <dir>     C include search directory");
    println!("  --cflag <flag>        Raw flag passed to the C compiler");
    println!("  --c-lib <name>        Native library to link");
    println!("  --define <name[=val]> Preprocessor define");
    println!();
    println!("Security (the [security] section, without a config file):");
    println!("  --security-mode <m>   open | strict");
    println!("  --extern <pattern>    Allowed extern symbol, '*' suffix allowed");
    println!("  --extern-lib <name>   Allowed extern library");
    println!("  --allow-macro <name>  Allowed macro");
    println!("  --allow-unsafe        Permit unsafe blocks");
    println!("  --macro <name=path>   Bind a macro name to a source file");
    println!("\nWarnings (for build/run/test):");
    println!("  --show-warn           Show warning summary");
    println!("  --position            Show per-file warning locations");
    println!("  --no-warn <cat>       Suppress warning category (repeatable)");
    println!("  -W <code>             Promote warning to error");
    println!("  --deny <code>         Deny specific warning code");
    println!("\nCommands:");
    println!("  run [file] [-- args]  Compile + execute");
    println!("  build [file]          Compile only");
    println!("  debug [file]          Debug build, emits IR");
    println!("  test [paths...]       Run integration tests from tests/");
    println!("    --no-run            Compile only, skip execution");
    println!("    --verbose, -v       Show per-test results");
    println!("    --show-warn         Show warning summary");
    println!("    --position          Show per-file warning locations");
    println!("    --jobs, -j <n>      Parallel compilation jobs (0 = logical CPUs)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_options_parse_format_and_color() {
        let args = vec![
            "mire".to_string(),
            "build".to_string(),
            "--color".to_string(),
            "never".to_string(),
            "--error-format".to_string(),
            "json".to_string(),
        ];
        let (format, color) = output_options(&args);
        assert_eq!(format, ErrorFormat::Json);
        assert_eq!(color, ColorPolicy::Never);
        assert!(!color.enabled());
    }
}
