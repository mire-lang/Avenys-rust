use crate::error::diagnostic::{Diagnostic, DiagnosticCode, LabelStyle, Severity};
use serde_json::{Value, json};
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorFormat {
    #[default]
    Human,
    Short,
    Json,
    JsonRenderedAnsi,
    Sarif,
}

impl ErrorFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "human" => Some(Self::Human),
            "short" => Some(Self::Short),
            "json" => Some(Self::Json),
            "json-rendered-ansi" | "json-rendered" => Some(Self::JsonRenderedAnsi),
            "sarif" => Some(Self::Sarif),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Short => "short",
            Self::Json => "json",
            Self::JsonRenderedAnsi => "json-rendered-ansi",
            Self::Sarif => "sarif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPolicy {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    pub fn enabled(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

pub fn format_diagnostic_for(diag: &Diagnostic, format: ErrorFormat, use_color: bool) -> String {
    match format {
        ErrorFormat::Human => format_diagnostic(diag, use_color),
        ErrorFormat::Short => format_short(diag),
        ErrorFormat::Json => {
            let rendered = None;
            serde_json::to_string_pretty(&diagnostic_json(diag, rendered))
                .unwrap_or_else(|_| "{}".to_string())
        }
        ErrorFormat::JsonRenderedAnsi => {
            let rendered = Some(format_diagnostic(diag, true));
            serde_json::to_string_pretty(&diagnostic_json(diag, rendered))
                .unwrap_or_else(|_| "{}".to_string())
        }
        ErrorFormat::Sarif => {
            serde_json::to_string_pretty(&sarif_json_for(std::slice::from_ref(diag)))
                .unwrap_or_else(|_| "{}".to_string())
        }
    }
}

pub fn format_diagnostics_for(
    diags: &[Diagnostic],
    format: ErrorFormat,
    use_color: bool,
) -> String {
    if diags.is_empty() {
        return String::new();
    }
    match format {
        ErrorFormat::Human => diags
            .iter()
            .map(|d| format_diagnostic(d, use_color))
            .collect::<Vec<_>>()
            .join("\n"),
        ErrorFormat::Short => diags
            .iter()
            .map(format_short)
            .collect::<Vec<_>>()
            .join("\n"),
        ErrorFormat::Json | ErrorFormat::JsonRenderedAnsi => {
            let rendered = matches!(format, ErrorFormat::JsonRenderedAnsi);
            let values: Vec<Value> = diags
                .iter()
                .map(|d| diagnostic_json(d, rendered.then(|| format_diagnostic(d, true))))
                .collect();
            serde_json::to_string_pretty(&Value::Array(values)).unwrap_or_else(|_| "[]".to_string())
        }
        ErrorFormat::Sarif => serde_json::to_string_pretty(&sarif_json_for(diags))
            .unwrap_or_else(|_| "{}".to_string()),
    }
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
        Severity::Help => "help",
    }
}

fn severity_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note | Severity::Help => "note",
    }
}

fn format_short(diag: &Diagnostic) -> String {
    let filename = diag.filename.as_deref().unwrap_or("<unknown>");
    let location = if diag.span.is_unknown() {
        filename.to_string()
    } else {
        format!(
            "{}:{}:{}",
            filename,
            diag.span.line.max(1),
            diag.span.column.max(1)
        )
    };
    let message = diag.message.replace('\n', " ");
    format!("{}: {}: {}", location, severity_str(diag.severity), message)
}

fn diagnostic_json(diag: &Diagnostic, rendered: Option<String>) -> Value {
    let filename = diag.filename.as_deref().unwrap_or("");
    let labels: Vec<Value> = diag
        .labels
        .iter()
        .map(|label| {
            json!({
                "message": label.message,
                "style": match label.style {
                    LabelStyle::Primary => "primary",
                    LabelStyle::Secondary => "secondary",
                },
                "line": label.span.line,
                "column": label.span.column,
                "length": label.length,
            })
        })
        .collect();
    let suggestions: Vec<Value> = diag
        .suggestions
        .iter()
        .map(|suggestion| {
            json!({
                "message": suggestion.message,
                "replacement": suggestion.replacement,
            })
        })
        .collect();
    let mut value = json!({
        "severity": severity_str(diag.severity),
        "code": diag.code.as_str(),
        "name": diag.code.name(),
        "title": diag.title,
        "message": diag.message,
        "file": filename,
        "line": diag.span.line,
        "column": diag.span.column,
        "labels": labels,
        "notes": diag.notes,
        "help": diag.help,
        "suggestions": suggestions,
    });
    if let Some(rendered) = rendered {
        value["rendered"] = Value::String(rendered);
    }
    value
}

fn sarif_json_for(diags: &[Diagnostic]) -> Value {
    let mut rules: Vec<Value> = Vec::new();
    let mut results: Vec<Value> = Vec::new();
    for diag in diags {
        let filename = diag.filename.as_deref().unwrap_or("");
        let code = diag.code.as_str();
        if !rules.iter().any(|rule| rule["id"].as_str() == Some(code)) {
            let mut rule = json!({
                "id": code,
                "name": diag.code.name(),
                "shortDescription": { "text": diag.title },
            });
            if let Some(help) = &diag.help {
                rule["help"] = json!({ "text": help });
            }
            rules.push(rule);
        }

        let mut result = json!({
            "ruleId": code,
            "level": severity_level(diag.severity),
            "message": { "text": diag.message },
        });
        if !filename.is_empty() || !diag.span.is_unknown() {
            let mut region = json!({});
            if !diag.span.is_unknown() {
                region["startLine"] = json!(diag.span.line.max(1));
                region["startColumn"] = json!(diag.span.column.max(1));
            }
            let mut physical = json!({});
            if !filename.is_empty() {
                physical["artifactLocation"] = json!({ "uri": filename });
            }
            physical["region"] = region;
            result["locations"] = json!([{ "physicalLocation": physical }]);
        }
        if !diag.notes.is_empty() {
            result["properties"] = json!({ "notes": diag.notes });
        }
        results.push(result);
    }

    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "mire",
                    "informationUri": "https://mire-lang.github.io",
                    "rules": rules,
                }
            },
            "results": results,
        }],
    })
}

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
    let sev_prefix = if use_color { sev_color } else { "" };

    out.push_str(&format!(
        "{sev_prefix}{sev_word}[{code}]{} ── {}{}\n",
        c(use_color, "\x1b[0m"),
        diag.title,
        c(use_color, "\x1b[0m"),
        code = code_label,
    ));

    let display_line = span.line.max(1);
    let display_col = span.column.max(1);
    if span.is_unknown() {
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
        out.push_str("│     │ <no source location available>\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{DiagnosticCode, Span};

    fn sample() -> Diagnostic {
        let mut diag = Diagnostic::new(
            Severity::Error,
            DiagnosticCode::E0003,
            "Syntax Error",
            "expected expression",
            Span::new(2, 3),
        );
        diag.filename = Some("bad.mire".to_string());
        diag
    }

    #[test]
    fn short_format_is_one_line_with_location() {
        let rendered = format_diagnostic_for(&sample(), ErrorFormat::Short, false);
        assert_eq!(rendered, "bad.mire:2:3: error: expected expression");
    }

    #[test]
    fn json_format_exposes_code_and_span() {
        let rendered = format_diagnostic_for(&sample(), ErrorFormat::Json, false);
        let value: Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(value["code"], "E0003");
        assert_eq!(value["line"], 2);
        assert_eq!(value["column"], 3);
    }

    #[test]
    fn sarif_format_is_versioned() {
        let rendered = format_diagnostic_for(&sample(), ErrorFormat::Sarif, false);
        let value: Value = serde_json::from_str(&rendered).expect("valid sarif");
        assert_eq!(value["version"], "2.1.0");
        assert_eq!(value["runs"][0]["results"][0]["ruleId"], "E0003");
    }

    #[test]
    fn multiple_json_diagnostics_form_an_array() {
        let diags = vec![sample(), sample()];
        let rendered = format_diagnostics_for(&diags, ErrorFormat::Json, false);
        let value: Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(value.as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn format_and_color_parse_known_values() {
        assert_eq!(ErrorFormat::parse("sarif"), Some(ErrorFormat::Sarif));
        assert_eq!(ErrorFormat::parse("nope"), None);
        assert_eq!(ColorPolicy::parse("never"), Some(ColorPolicy::Never));
    }
}
