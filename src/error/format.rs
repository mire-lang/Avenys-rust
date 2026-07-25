use crate::error::diagnostic::{Diagnostic, DiagnosticCode, LabelStyle, Severity};

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
    let span = primary.map(|label| label.span).unwrap_or(diag.span);

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

    let display_line = span.line.max(1);
    let display_col = span.column.max(1);
    if span.is_unknown() && diag.filename.is_some() {
        out.push_str(&format!(
            "{}╭─[ {} ]{}\n",
            c(use_color, "\x1b[1;36m"),
            filename,
            c(use_color, "\x1b[0m")
        ));
    } else if span.is_unknown() {
        out.push_str(&format!(
            "{}╭─[ {} ]{}\n",
            c(use_color, "\x1b[1;36m"),
            filename,
            c(use_color, "\x1b[0m")
        ));
    } else {
        out.push_str(&format!(
            "{}╭─[ {}:{}:{} ]{}\n",
            c(use_color, "\x1b[1;36m"),
            filename,
            display_line,
            display_col,
            c(use_color, "\x1b[0m")
        ));
    }

    if !span.is_unknown() {
        // Source text resolution: try diag.source first, then read the file.
        if let Some(source) = &diag.source {
            render_source_lines(&mut out, source, span, diag);
        } else if let Some(ref fname) = diag.filename {
            let path = std::path::Path::new(fname);
            match std::fs::read_to_string(path) {
                Ok(content) => render_source_lines(&mut out, &content, span, diag),
                Err(_) => out.push_str("│     │ <no source text available>\n"),
            }
        } else {
            out.push_str("│     │ <no source text available>\n");
        }
    } else {
        out.push_str(&format!(
            "│     │ <no source location available>\n"
        ));
    }

    out.push_str(&format!("╰─ {}\n", diag.message));
    if span.is_unknown() && diag.filename.is_none() && diag.code != DiagnosticCode::E0017 {
        out.push_str("   ─┬─ note: toolchain error (no source location available)\n");
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

fn render_source_lines(
    out: &mut String,
    source: &str,
    span: crate::error::Span,
    diag: &Diagnostic,
) {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        out.push_str("│     │ <no source text available>\n");
        return;
    }
    let line = span.line;
    let start = line.saturating_sub(2).max(1);
    let end = (line + 2).min(lines.len());
    let width = end.to_string().len();

    for lno in start..=end {
        let txt = lines.get(lno - 1).copied().unwrap_or("");
        out.push_str(&format!("│ {:>width$} │ {}\n", lno, txt, width = width));
        for label in diag.labels.iter().filter(|x| x.span.line == lno) {
            let marker = match label.style {
                LabelStyle::Primary => '^',
                LabelStyle::Secondary => '-',
            };
            let marker_len = label.length.max(1);
            out.push_str(&format!(
                "│ {:>width$} │ {}{} {}\n",
                "",
                " ".repeat(label.span.column.saturating_sub(1)),
                marker.to_string().repeat(marker_len),
                label.message,
                width = width
            ));
        }
    }
}
