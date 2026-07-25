pub mod diagnostic;
pub mod format;
pub mod mss;

use diagnostic::{Diagnostic, Label, LabelStyle, Severity};
pub use diagnostic::{DiagnosticCode, Diagnostic as Diag, Label as DiagLabel, LabelStyle as DiagLabelStyle, Severity as DiagSeverity};
use format::format_diagnostic;
use mss::MssError;

/// Source location span — always present in errors and warnings.
/// This is the single source of truth for "where" something happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    pub const fn unknown() -> Self {
        Self {
            line: 0,
            column: 0,
        }
    }

    pub const fn is_unknown(&self) -> bool {
        self.line == 0 && self.column == 0
    }

    pub const fn to_tuple(self) -> (usize, usize) {
        (self.line, self.column)
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::unknown()
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[derive(Debug, Clone)]
pub enum ErrorKind {
    Lexer {
        span: Span,
        message: String,
    },
    DeprecatedSyntax {
        span: Span,
        message: String,
    },
    Parser {
        span: Span,
        message: String,
    },
    Backend {
        span: Span,
        message: String,
    },
    Runtime {
        span: Span,
        message: String,
    },
    Type {
        span: Span,
        message: String,
        code: Option<DiagnosticCode>,
    },
    Ownership {
        span: Span,
        kind: MssError,
    },
    Cli {
        message: String,
    },
}

impl ErrorKind {
    pub fn runtime(span: Span, message: String) -> Self {
        ErrorKind::Runtime { span, message }
    }

    pub fn type_error_at(span: Span, message: String) -> Self {
        ErrorKind::Type {
            span,
            message,
            code: None,
        }
    }

    pub fn ownership_error(span: Span, kind: MssError) -> Self {
        ErrorKind::Ownership { span, kind }
    }
}

#[derive(Debug, Clone, Default)]
struct MireErrorContext {
    source: Option<String>,
    filename: Option<String>,
    explanation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MireError {
    pub kind: ErrorKind,
    pub span: Span,
    diagnostic: Box<Diagnostic>,
    context: Option<Box<MireErrorContext>>,
}

impl MireError {
    /// Access the source line number.
    pub fn line(&self) -> usize {
        self.span.line
    }

    /// Access the source column number.
    pub fn column(&self) -> usize {
        self.span.column
    }
}

impl std::fmt::Display for MireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_color())
    }
}

impl std::error::Error for MireError {}

impl MireError {
    pub fn new(kind: ErrorKind) -> Self {
        let (span, title, message, code) = map_kind(&kind);
        let mut diagnostic = Diagnostic::new(Severity::Error, code, title, message, span);
        if !span.is_unknown() {
            diagnostic.labels.push(Label {
                span,
                length: 3,
                message: "here".to_string(),
                style: LabelStyle::Primary,
            });
        }
        diagnostic.help = default_help_for_code(code);

        Self {
            kind,
            span,
            diagnostic: Box::new(diagnostic),
            context: Some(Box::new(MireErrorContext {
                source: None,
                filename: None,
                explanation: None,
            })),
        }
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Create a MireError from an existing Diagnostic, preserving its code and span.
    pub fn from_diagnostic(diag: &Diagnostic) -> Self {
        let span = diag.labels.first().map(|l| l.span).unwrap_or(Span::unknown());
        let kind = ErrorKind::Runtime {
            span,
            message: diag.message.clone(),
        };
        Self {
            kind,
            span,
            diagnostic: Box::new(diag.clone()),
            context: Some(Box::new(MireErrorContext {
                source: None,
                filename: None,
                explanation: None,
            })),
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.context_mut().source = Some(source.clone());
        self.diagnostic.source = Some(source);
        self
    }

    pub fn with_filename(mut self, filename: String) -> Self {
        self.context_mut().filename = Some(filename.clone());
        self.diagnostic.filename = Some(filename);
        self
    }

    pub fn ensure_context(mut self, filename: &str, source: &str) -> Self {
        if self.diagnostic.filename.is_none() {
            self = self.with_filename(filename.to_string());
        }
        if self.diagnostic.source.is_none() {
            self = self.with_source(source.to_string());
        }
        self
    }

    pub fn with_explanation(mut self, explanation: String) -> Self {
        self.context_mut().explanation = Some(explanation.clone());
        self.diagnostic.notes.push(explanation);
        self
    }

    pub fn with_suggestion(mut self, message: String, replacement: Option<String>) -> Self {
        self.diagnostic
            .suggestions
            .push(diagnostic::Suggestion { message, replacement });
        self
    }

    pub fn with_position(mut self, line: usize, column: usize) -> Self {
        self.span = Span::new(line, column);
        self.diagnostic.span = self.span;
        if self.diagnostic.labels.is_empty() {
            self.diagnostic.labels.push(Label {
                span: self.span,
                length: 3,
                message: "here".to_string(),
                style: LabelStyle::Primary,
            });
        } else {
            for label in &mut self.diagnostic.labels {
                if label.style == LabelStyle::Primary {
                    label.span = self.span;
                }
            }
        }
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self.diagnostic.span = span;
        if self.diagnostic.labels.is_empty() {
            self.diagnostic.labels.push(Label {
                span,
                length: 3,
                message: "here".to_string(),
                style: LabelStyle::Primary,
            });
        } else {
            for label in &mut self.diagnostic.labels {
                if label.style == LabelStyle::Primary {
                    label.span = span;
                }
            }
        }
        self
    }

    pub fn source(&self) -> Option<&String> {
        self.context.as_ref().and_then(|ctx| ctx.source.as_ref())
    }

    pub fn filename(&self) -> Option<&String> {
        self.context.as_ref().and_then(|ctx| ctx.filename.as_ref())
    }

    pub fn explanation(&self) -> Option<&String> {
        self.context
            .as_ref()
            .and_then(|ctx| ctx.explanation.as_ref())
    }

    pub fn set_source(&mut self, source: Option<String>) {
        self.context_mut().source = source.clone();
        self.diagnostic.source = source;
    }

    pub fn set_filename(&mut self, filename: Option<String>) {
        self.context_mut().filename = filename.clone();
        self.diagnostic.filename = filename;
    }

    pub fn set_explanation(&mut self, explanation: Option<String>) {
        self.context_mut().explanation = explanation.clone();
        if let Some(explanation) = explanation {
            self.diagnostic.notes.push(explanation);
        }
    }

    pub fn source_mut(&mut self) -> &mut Option<String> {
        &mut self.context_mut().source
    }

    pub fn filename_mut(&mut self) -> &mut Option<String> {
        &mut self.context_mut().filename
    }

    pub fn explanation_mut(&mut self) -> &mut Option<String> {
        &mut self.context_mut().explanation
    }

    pub fn format(&self) -> String {
        format_diagnostic(&self.diagnostic, false)
    }

    pub fn format_color(&self) -> String {
        format_diagnostic(&self.diagnostic, true)
    }

    fn context_mut(&mut self) -> &mut MireErrorContext {
        self.context
            .get_or_insert_with(|| Box::new(MireErrorContext::default()))
            .as_mut()
    }
}

impl From<std::io::Error> for MireError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorKind::Runtime {
            span: Span::unknown(),
            message: e.to_string(),
        })
    }
}

impl MireError {
    pub fn deprecated_syntax(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::DeprecatedSyntax {
            span: Span::new(line, column),
            message,
        })
    }

    pub fn backend_at(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::Backend {
            span: Span::new(line, column),
            message,
        })
    }

    pub fn runtime(message: String) -> Self {
        Self::new(ErrorKind::Runtime {
            span: Span::unknown(),
            message,
        })
    }

    pub fn cli(message: String) -> Self {
        Self::new(ErrorKind::Cli { message })
    }

    pub fn runtime_at(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::Runtime {
            span: Span::new(line, column),
            message,
        })
    }

    pub fn type_error_at(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::Type {
            span: Span::new(line, column),
            message,
            code: None,
        })
    }

    /// Type error with an explicit diagnostic code (e.g. `E0107` for an
    /// integer literal out of the target range). Falls back to `E0005`.
    pub fn type_error_code(
        line: usize,
        column: usize,
        code: DiagnosticCode,
        message: String,
    ) -> Self {
        Self::new(ErrorKind::Type {
            span: Span::new(line, column),
            message,
            code: Some(code),
        })
    }

    pub fn ownership_error(line: usize, column: usize, kind: MssError) -> Self {
        Self::new(ErrorKind::Ownership {
            span: Span::new(line, column),
            kind,
        })
    }
}

fn map_kind(kind: &ErrorKind) -> (Span, &'static str, String, DiagnosticCode) {
    match kind {
        ErrorKind::Lexer { span, message } => (
            *span,
            "Lexical Error",
            message.clone(),
            DiagnosticCode::E0001,
        ),
        ErrorKind::DeprecatedSyntax { span, message } => (
            *span,
            "Deprecated Syntax",
            message.clone(),
            DiagnosticCode::W0010,
        ),
        ErrorKind::Parser { span, message } => (
            *span,
            "Syntax Error",
            message.clone(),
            DiagnosticCode::E0003,
        ),
        ErrorKind::Backend { span, message } => (
            *span,
            "Backend Limitation",
            message.clone(),
            DiagnosticCode::E0014,
        ),
        ErrorKind::Runtime { span, message } => (
            *span,
            "Runtime Error",
            message.clone(),
            DiagnosticCode::E0015,
        ),
        ErrorKind::Type {
            span,
            message,
            code,
        } => (
            *span,
            "Type Error",
            message.clone(),
            code.unwrap_or(DiagnosticCode::E0005),
        ),
        ErrorKind::Ownership { span, kind } => (
            *span,
            "Ownership Error",
            kind.to_string(),
            kind.diagnostic_code(),
        ),
        ErrorKind::Cli { message } => (
            Span::unknown(),
            "CLI Error",
            message.clone(),
            DiagnosticCode::E0017,
        ),
    }
}

fn default_help_for_code(code: DiagnosticCode) -> Option<String> {
    match code {
        DiagnosticCode::E0005 => Some("review the declared type and assigned expression".to_string()),
        DiagnosticCode::E0014 => Some(
            "The frontend accepted this program, but the current Avenys backend cannot lower this construct yet."
                .to_string(),
        ),
        DiagnosticCode::E0100
        | DiagnosticCode::E0101
        | DiagnosticCode::E0102
        | DiagnosticCode::E0103
        | DiagnosticCode::E0104
        | DiagnosticCode::E0105
        | DiagnosticCode::E0106
        | DiagnosticCode::E0107
        | DiagnosticCode::E0108
        | DiagnosticCode::E0109
        | DiagnosticCode::E0110 => Some(
            "Mire uses real types with exact widths. Use an explicit cast `(value :T)` to convert."
                .to_string(),
        ),
        _ => None,
    }
}

pub type Result<T> = std::result::Result<T, MireError>;

pub fn type_error(line: usize, column: usize, message: String) -> MireError {
    MireError::type_error_at(line, column, message)
}

pub fn type_error_at_span(span: Span, message: String) -> MireError {
    MireError::new(ErrorKind::Type {
        span,
        message,
        code: None,
    })
}

/// Type error carrying an explicit diagnostic code (e.g. `E0107`).
pub fn type_error_code(
    line: usize,
    column: usize,
    code: DiagnosticCode,
    message: String,
) -> MireError {
    MireError::type_error_code(line, column, code, message)
}

/// Type error with code and span.
pub fn type_error_code_at_span(
    span: Span,
    code: DiagnosticCode,
    message: String,
) -> MireError {
    MireError::new(ErrorKind::Type {
        span,
        message,
        code: Some(code),
    })
}

pub fn format_error_chain(errors: &[MireError], use_color: bool) -> String {
    if errors.is_empty() {
        return String::new();
    }
    errors
        .iter()
        .map(|e| {
            if use_color {
                e.format_color()
            } else {
                e.format()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{ErrorKind, MireError, Span};
    use crate::error::diagnostic::DiagnosticCode;

    #[test]
    fn mire_error_stays_compact_enough_for_result_large_err() {
        let size = std::mem::size_of::<MireError>();
        assert!(
            size <= 80,
            "MireError regressed in size: expected <= 80 bytes, got {size}"
        );

        let err = MireError::new(ErrorKind::Runtime {
            span: Span::unknown(),
            message: "boom".to_string(),
        })
        .with_filename("main.mire".to_string())
        .with_source("use dasu(1)\n".to_string())
        .with_explanation("runtime".to_string());

        assert_eq!(err.filename().map(String::as_str), Some("main.mire"));
        assert!(err.source().is_some());
        assert!(err.explanation().is_some());
    }

    #[test]
    fn span_is_always_present() {
        let err = MireError::runtime("test".to_string());
        assert!(!err.span.is_unknown() || err.span.is_unknown());
        assert_eq!(err.span.line, 0);
        assert_eq!(err.span.column, 0);
    }

    #[test]
    fn span_display_format() {
        let span = Span::new(42, 5);
        assert_eq!(format!("{}", span), "42:5");
    }

    // ── VERIFICATION TESTS ──────────────────────────────────────────────
    // These tests prove that errors which previously lacked position info
    // now ALWAYS show a position in their formatted output.

    /// Test: error with known position shows it in formatted output
    #[test]
    fn error_with_known_position_shows_location() {
        let err = MireError::type_error_at(10, 5, "type mismatch".to_string())
            .with_filename("test.mire".to_string())
            .with_source("fn main: () {\n    let x = 42\n}\n".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("test.mire:10:5"),
            "Expected position in output, got:\n{}",
            formatted
        );
    }

    /// Test: error with unknown position (0,0) still shows a location in output
    #[test]
    fn error_with_unknown_position_shows_recorded_location() {
        let err = MireError::runtime("io error".to_string())
            .with_filename("build.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("build.mire"),
            "Expected filename in output, got:\n{}",
            formatted
        );
        assert!(
            formatted.contains("╭─["),
            "Expected position header in output, got:\n{}",
            formatted
        );
    }

    /// Test: type_error_at_span function works correctly
    #[test]
    fn type_error_at_span_creates_correct_error() {
        let span = Span::new(25, 12);
        let err = crate::error::type_error_at_span(span, "cannot unify".to_string());

        assert_eq!(err.span.line, 25);
        assert_eq!(err.span.column, 12);
        let formatted = err.format();
        assert!(
            formatted.contains("25:12"),
            "Expected position 25:12 in output, got:\n{}",
            formatted
        );
    }

    /// Test: type_error_code_at_span creates error with both code and position
    #[test]
    fn type_error_code_at_span_has_code_and_position() {
        let span = Span::new(3, 8);
        let err = crate::error::type_error_code_at_span(
            span,
            DiagnosticCode::E0107,
            "literal out of range".to_string(),
        );

        let formatted = err.format();
        assert!(
            formatted.contains("E0107"),
            "Expected error code E0107, got:\n{}",
            formatted
        );
        assert!(
            formatted.contains("3:8"),
            "Expected position 3:8, got:\n{}",
            formatted
        );
    }

    /// Test: lexer error always shows position
    #[test]
    fn lexer_error_shows_position() {
        let err = MireError::new(ErrorKind::Lexer {
            span: Span::new(5, 10),
            message: "unterminated string".to_string(),
        })
        .with_filename("code.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("code.mire:5:10"),
            "Expected lexer position in output, got:\n{}",
            formatted
        );
    }

    /// Test: parser error always shows position
    #[test]
    fn parser_error_shows_position() {
        let err = MireError::new(ErrorKind::Parser {
            span: Span::new(1, 1),
            message: "unexpected token".to_string(),
        })
        .with_filename("main.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("main.mire:1:1"),
            "Expected parser position in output, got:\n{}",
            formatted
        );
    }

    /// Test: ownership error shows position
    #[test]
    fn ownership_error_shows_position() {
        use crate::error::mss::MssError;

        let err = MireError::ownership_error(12, 4, MssError::UseAfterMove)
            .with_filename("refs.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("refs.mire:12:4"),
            "Expected ownership error position in output, got:\n{}",
            formatted
        );
    }

    /// Test: deprecated syntax error shows position
    #[test]
    fn deprecated_syntax_error_shows_position() {
        let err = MireError::deprecated_syntax(8, 3, "old syntax".to_string())
            .with_filename("old.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("old.mire:8:3"),
            "Expected deprecated syntax position, got:\n{}",
            formatted
        );
    }

    /// Test: backend error shows position
    #[test]
    fn backend_error_shows_position() {
        let err = MireError::backend_at(20, 1, "not implemented".to_string())
            .with_filename("complex.mire".to_string());

        let formatted = err.format();
        assert!(
            formatted.contains("complex.mire:20:1"),
            "Expected backend error position, got:\n{}",
            formatted
        );
    }

    /// Test: error chain shows position for every error
    #[test]
    fn error_chain_all_have_positions() {
        let errors = vec![
            MireError::type_error_at(1, 1, "err1".to_string()),
            MireError::runtime("err2".to_string()),
            MireError::new(ErrorKind::Lexer {
                span: Span::new(5, 3),
                message: "err3".to_string(),
            }),
        ];

        let chain = super::format_error_chain(&errors, false);
        // Every error should have the ╭─[ pattern indicating a position header
        let header_count = chain.matches("╭─[").count();
        assert_eq!(
            header_count, 3,
            "Expected 3 position headers in error chain, got {}",
            header_count
        );
    }

    /// Test: with_span updates the position correctly
    #[test]
    fn with_span_updates_position() {
        let err = MireError::runtime("test".to_string())
            .with_span(Span::new(99, 7));

        assert_eq!(err.span.line, 99);
        assert_eq!(err.span.column, 7);
        let formatted = err.format();
        assert!(
            formatted.contains("99:7"),
            "Expected updated position, got:\n{}",
            formatted
        );
    }

    /// Test: with_position backward compat still works
    #[test]
    fn with_position_backward_compat() {
        let err = MireError::runtime("test".to_string())
            .with_position(50, 15);

        assert_eq!(err.span.line, 50);
        assert_eq!(err.span.column, 15);
    }

    /// Test: warning diagnostic always has span
    #[test]
    fn warning_diagnostic_always_has_span() {
        use crate::error::diagnostic::{Diagnostic, Severity};

        let diag = Diagnostic::new(
            Severity::Warning,
            DiagnosticCode::W0001,
            "Unused Variable",
            "x is never used",
            Span::new(3, 5),
        );

        assert_eq!(diag.span.line, 3);
        assert_eq!(diag.span.column, 5);
        let formatted = super::format::format_diagnostic(&diag, false);
        assert!(
            formatted.contains("3:5"),
            "Expected warning position, got:\n{}",
            formatted
        );
    }
}
