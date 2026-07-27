use super::warnings::WarningAnalyzer;
use crate::error::diagnostic::{Diagnostic, DiagnosticCode, Label, LabelStyle, Severity};
use crate::parser::Program;
use crate::parser::ast::{Expression, Statement};
use std::collections::HashSet;

impl WarningAnalyzer {
    pub(super) fn push_warn(
        &mut self,
        code: DiagnosticCode,
        title: &str,
        message: String,
        span: crate::error::Span,
        help: Option<String>,
    ) {
        self.push_warn_at(code, title, message, span, 3, help);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_warn_at(
        &mut self,
        code: DiagnosticCode,
        title: &str,
        message: String,
        span: crate::error::Span,
        length: usize,
        help: Option<String>,
    ) {
        if !self.filter.matches(code) {
            return;
        }
        let severity = if self.deny.contains(&code) {
            Severity::Error
        } else {
            Severity::Warning
        };
        let mut diag = Diagnostic::new(severity, code, title, message, span);
        diag.labels.push(Label {
            span,
            length,
            message: String::new(),
            style: LabelStyle::Primary,
        });
        diag.help = help;
        self.diagnostics.push(diag);
    }

    pub(super) fn warn_duplicate_literal_patterns(
        &mut self,
        cases: &[(Expression, Vec<Statement>)],
    ) {
        let mut seen = HashSet::new();
        for (pat, _) in cases {
            if let Some(key) = super::warnings::literal_pattern_key(pat)
                && !seen.insert(key.clone())
            {
                self.push_warn(
                    DiagnosticCode::W0038,
                    "Duplicate Match Pattern",
                    format!("Duplicate literal pattern '{}' in match", key),
                    self.current_span,
                    Some("remove the duplicate pattern or merge with the first one".to_string()),
                );
            }
        }
    }

    pub(super) fn check_deny_unsafe(&mut self, program: &Program, filename: Option<&str>) {
        let file_denies_unsafe = program
            .file_attributes
            .iter()
            .any(|a| a.name == "deny" && a.args.iter().any(|arg| arg.value == "unsafe"));

        for stmt in &program.statements {
            if let Statement::Function {
                name,
                body,
                attributes,
                ..
            } = stmt
            {
                let function_denies = attributes
                    .iter()
                    .any(|a| a.name == "deny" && a.args.iter().any(|arg| arg.value == "unsafe"));
                if !file_denies_unsafe && !function_denies {
                    continue;
                }
                if let Some(loc) = super::warnings::find_unsafe_block_position(body) {
                    let mut diag = Diagnostic::new(
                        Severity::Error,
                        DiagnosticCode::E0016,
                        "unsafe not allowed",
                        format!(
                            "Function '{}' contains an unsafe block but @[deny(unsafe)] forbids it",
                            name
                        ),
                        loc,
                    );
                    diag.labels.push(Label {
                        span: loc,
                        length: 6,
                        message: "unsafe block here".to_string(),
                        style: LabelStyle::Primary,
                    });
                    diag.help = Some(
                        "remove the unsafe block or remove the @[deny(unsafe)] attribute"
                            .to_string(),
                    );
                    if let Some(filename) = filename {
                        diag.filename = Some(filename.to_string());
                    }
                    self.diagnostics.push(diag);
                }
            }
        }
    }
}
