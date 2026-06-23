use crate::error::diagnostic::{Diagnostic, LabelStyle, Severity};

fn c(use_color: bool, s: &str) -> &str {
    if use_color { s } else { "" }
}

pub fn format_diagnostic(diag: &Diagnostic, use_color: bool) -> String {
    let (sev_word, sev_color) = match diag.severity {
        Severity::Error => ("error", "\x1b[1;31m"),
        Severity::Warning => ("warning", "\x1b[1;33m"),
        Severity::Note => ("note", "\x1b[1;34m"),
        Severity::Help => ("help", "\x1b[1;32m"),
    };

    let filename = diag.filename.as_deref().unwrap_or("main.mire");
    let primary = diag
        .labels
        .iter()
        .find(|label| label.style == LabelStyle::Primary);
    let line = primary
        .map(|label| label.line.max(1))
        .unwrap_or_else(|| if diag.line == 0 { 1 } else { diag.line });
    let col = primary
        .map(|label| label.column.max(1))
        .unwrap_or_else(|| if diag.column == 0 { 1 } else { diag.column });
    let has_default_anchor = line == 1
        && col == 1
        && diag.labels.iter().any(|label| {
            label.style == LabelStyle::Primary && label.line <= 1 && label.column <= 1
        });

    let mut out = String::new();
    let code_label = match diag.severity {
        Severity::Warning | Severity::Note | Severity::Help => {
            format!("{}::{}", diag.code.as_str(), diag.code.name())
        }
        Severity::Error => diag.code.as_str().to_string(),
    };

    out.push_str(&format!(
        "{sev_color}{sev_word}[{code}]{} ── {}{}\n",
        c(use_color, "\x1b[0m"),
        diag.title,
        c(use_color, "\x1b[0m"),
        code = code_label,
    ));
    out.push_str(&format!(
        "{}╭─[ {}:{}:{} ]{}\n",
        c(use_color, "\x1b[1;36m"),
        filename,
        line,
        col,
        c(use_color, "\x1b[0m")
    ));

    if !has_default_anchor && let Some(source) = &diag.source {
        let lines: Vec<&str> = source.lines().collect();
        if !lines.is_empty() {
            let start = line.saturating_sub(2).max(1);
            let end = (line + 2).min(lines.len());
            let width = end.to_string().len();

            for lno in start..=end {
                let txt = lines.get(lno - 1).copied().unwrap_or("");
                out.push_str(&format!("│ {:>width$} │ {}\n", lno, txt, width = width));
                for label in diag.labels.iter().filter(|x| x.line == lno) {
                    let marker = match label.style {
                        LabelStyle::Primary => '^',
                        LabelStyle::Secondary => '-',
                    };
                    let marker_len = label.length.max(1);
                    out.push_str(&format!(
                        "│ {:>width$} │ {}{} {}\n",
                        "",
                        " ".repeat(label.column.saturating_sub(1)),
                        marker.to_string().repeat(marker_len),
                        label.message,
                        width = width
                    ));
                }
            }
        }
    }
    if has_default_anchor {
        out.push_str("│     │ <source location unavailable>\n");
    }

    out.push_str(&format!("╰─ {}\n", diag.message));
    if has_default_anchor {
        out.push_str("   ─┬─ note: error location is approximate; emitted from non-positioned backend/runtime path\n");
    }
    for note in &diag.notes {
        out.push_str(&format!("   ─┬─ note: {}\n", note));
    }
    if let Some(help) = &diag.help {
        out.push_str(&format!("   ─┬─ help: {}\n", help));
    }
    for suggestion in &diag.suggestions {
        out.push_str(&format!("   ─┬─ suggestion: {}\n", suggestion.message));
    }
    out
}
