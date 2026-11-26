use mire::BuildResult;
use mire::error::diagnostic::Diagnostic;
use mire::error::format::{
    ColorPolicy, ErrorFormat, format_diagnostic_for, format_diagnostics_for,
};
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

/// Print one warning in the requested output format.
pub(crate) fn print_warning_detailed(d: &Diagnostic, format: ErrorFormat, color: ColorPolicy) {
    let rendered = format_diagnostic_for(d, format, color.enabled());
    eprint!("{}", rendered);
    if !rendered.ends_with('\n') {
        eprintln!();
    }
}

pub(crate) fn should_suppress(code_name: &str, suppressed: &[String]) -> bool {
    let hyphenated = code_name.replace('_', "-");
    suppressed
        .iter()
        .any(|s| s == code_name || s == &hyphenated)
}

/// Print warnings from a BuildResult according to `position` and the requested
/// output format.
pub(crate) fn emit_warnings(
    build: &BuildResult,
    position: bool,
    no_warn_cats: &[String],
    format: ErrorFormat,
    color: ColorPolicy,
) {
    let filtered: Vec<_> = build
        .warnings_raw
        .iter()
        .filter(|d| !should_suppress(d.code.name(), no_warn_cats))
        .cloned()
        .collect();
    if filtered.is_empty() {
        return;
    }

    if !matches!(format, ErrorFormat::Human) {
        if position {
            for d in &filtered {
                print_warning_detailed(d, format, color);
            }
        } else {
            eprintln!(
                "{}",
                format_diagnostics_for(&filtered, format, color.enabled())
            );
        }
        return;
    }

    if position {
        for d in &filtered {
            print_warning_detailed(d, format, color);
        }
    } else {
        print_warning_summary(&filtered);
    }
}
