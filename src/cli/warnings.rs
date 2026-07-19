use mire::BuildResult;
use mire::error::diagnostic::Diagnostic;
use mire::error::format::format_diagnostic;
use std::collections::BTreeMap;

/// Print warnings as a per-category summary table.
pub(crate) fn print_warning_summary(raw: &[Diagnostic]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in raw {
        let name = d.code.name().to_string();
        *counts.entry(name).or_insert(0) += 1;
    }
    if counts.is_empty() {
        return;
    }
    eprintln!("╭─[ warnings summary ]");
    let mut total = 0usize;
    for (i, (name, cnt)) in counts.iter().enumerate() {
        total += cnt;
        eprintln!(
            "│ {:>2} │  {} ............ {}",
            i + 1,
            name.replace('_', "-"),
            cnt,
        );
    }
    eprintln!(
        "│ {:>2} │  Total ............... {}",
        counts.len() + 1,
        total
    );
    eprintln!("╰─ Use --position (or --pos) to see per-file details.");
}

/// Print one warning in detailed format.
pub(crate) fn print_warning_detailed(d: &Diagnostic, use_color: bool) {
    eprint!("{}", format_diagnostic(d, use_color));
}

pub(crate) fn should_suppress(code_name: &str, suppressed: &[String]) -> bool {
    let hyphenated = code_name.replace('_', "-");
    suppressed.iter().any(|s| s == code_name || s == &hyphenated)
}

/// Print warnings from a BuildResult according to `position` flag.
pub(crate) fn emit_warnings(build: &BuildResult, position: bool, no_warn_cats: &[String]) {
    let filtered: Vec<_> = build
        .warnings_raw
        .iter()
        .filter(|d| !should_suppress(d.code.name(), no_warn_cats))
        .cloned()
        .collect();
    if position {
        for d in &filtered {
            print_warning_detailed(d, true);
        }
    } else {
        print_warning_summary(&filtered);
    }
}
