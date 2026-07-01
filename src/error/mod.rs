pub mod diagnostic;
pub mod format;
pub mod mss;

use diagnostic::{Diagnostic, DiagnosticCode, Label, LabelStyle, Severity};
use format::format_diagnostic;
use mss::MssError;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    Lexer {
        line: usize,
        column: usize,
        message: String,
    },
    DeprecatedSyntax {
        line: usize,
        column: usize,
        message: String,
    },
    Parser {
        line: usize,
        column: usize,
        message: String,
    },
    Backend {
        message: String,
    },
    Runtime {
        message: String,
    },
    Type {
        line: usize,
        column: usize,
        message: String,
    },
    Ownership {
        line: usize,
        column: usize,
        kind: MssError,
    },
}

impl ErrorKind {
    pub fn runtime(message: String) -> Self {
        ErrorKind::Runtime { message }
    }

    pub fn type_error(message: String) -> Self {
        ErrorKind::Type {
            line: 0,
            column: 0,
            message,
        }
    }

    pub fn type_error_at(line: usize, column: usize, message: String) -> Self {
        ErrorKind::Type {
            line,
            column,
            message,
        }
    }

    pub fn ownership_error(line: usize, column: usize, kind: MssError) -> Self {
        ErrorKind::Ownership { line, column, kind }
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
    pub line: usize,
    pub column: usize,
    diagnostic: Box<Diagnostic>,
    context: Option<Box<MireErrorContext>>,
}

impl std::fmt::Display for MireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_color())
    }
}

impl std::error::Error for MireError {}

impl MireError {
    pub fn new(kind: ErrorKind) -> Self {
        let (line, column, title, message, code) = map_kind(&kind);
        let mut diagnostic = Diagnostic::new(Severity::Error, code, title, message, line, column);
        if line > 0 {
            diagnostic.labels.push(Label {
                line,
                column,
                length: 3,
                message: "here".to_string(),
                style: LabelStyle::Primary,
            });
        }
        diagnostic.help = default_help_for_code(code);

        Self {
            kind,
            line,
            column,
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

    pub fn with_explanation(mut self, explanation: String) -> Self {
        self.context_mut().explanation = Some(explanation.clone());
        self.diagnostic.notes.push(explanation);
        self
    }

    pub fn with_position(mut self, line: usize, column: usize) -> Self {
        let line = line.max(1);
        let column = column.max(1);
        self.line = line;
        self.column = column;
        self.diagnostic.line = line;
        self.diagnostic.column = column;
        if self.diagnostic.labels.is_empty() {
            self.diagnostic.labels.push(Label {
                line,
                column,
                length: 3,
                message: "here".to_string(),
                style: LabelStyle::Primary,
            });
        } else {
            for label in &mut self.diagnostic.labels {
                if label.style == LabelStyle::Primary {
                    label.line = line;
                    label.column = column;
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
            message: e.to_string(),
        })
    }
}

impl MireError {
    pub fn deprecated_syntax(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::DeprecatedSyntax {
            line,
            column,
            message,
        })
    }

    pub fn backend(message: String) -> Self {
        Self::new(ErrorKind::Backend { message })
    }

    pub fn runtime(message: String) -> Self {
        Self::new(ErrorKind::Runtime { message })
    }

    pub fn runtime_at(line: usize, column: usize, message: String) -> Self {
        let mut error = Self::new(ErrorKind::Runtime { message });
        error.line = line;
        error.column = column;
        error.diagnostic.line = line;
        error.diagnostic.column = column;
        error
    }

    pub fn type_error(message: String) -> Self {
        Self::new(ErrorKind::Type {
            line: 0,
            column: 0,
            message,
        })
    }

    pub fn type_error_at(line: usize, column: usize, message: String) -> Self {
        Self::new(ErrorKind::Type {
            line,
            column,
            message,
        })
    }

    pub fn ownership_error(line: usize, column: usize, kind: MssError) -> Self {
        Self::new(ErrorKind::Ownership { line, column, kind })
    }
}

fn map_kind(kind: &ErrorKind) -> (usize, usize, &'static str, String, DiagnosticCode) {
    match kind {
        ErrorKind::Lexer {
            line,
            column,
            message,
        } => (
            *line,
            *column,
            "Lexical Error",
            message.clone(),
            DiagnosticCode::E0001,
        ),
        ErrorKind::DeprecatedSyntax {
            line,
            column,
            message,
        } => (
            *line,
            *column,
            "Deprecated Syntax",
            message.clone(),
            DiagnosticCode::W0010,
        ),
        ErrorKind::Parser {
            line,
            column,
            message,
        } => (
            *line,
            *column,
            "Syntax Error",
            message.clone(),
            DiagnosticCode::E0003,
        ),
        ErrorKind::Backend { message } => (
            1,
            1,
            "Backend Limitation",
            message.clone(),
            DiagnosticCode::E0014,
        ),
        ErrorKind::Runtime { message } => (
            1,
            1,
            "Runtime Error",
            message.clone(),
            DiagnosticCode::E0015,
        ),
        ErrorKind::Type {
            line,
            column,
            message,
        } => (
            *line,
            *column,
            "Type Error",
            message.clone(),
            DiagnosticCode::E0005,
        ),
        ErrorKind::Ownership { line, column, kind } => (
            *line,
            *column,
            "Ownership Error",
            kind.to_string(),
            kind.diagnostic_code(),
        ),
    }
}

fn default_help_for_code(code: DiagnosticCode) -> Option<String> {
    match code {
        DiagnosticCode::E0005 => Some("review the declared type and assigned expression".to_string()),
        DiagnosticCode::E0006 => Some("define the identifier before use".to_string()),
        DiagnosticCode::E0014 => Some(
            "The frontend accepted this program, but the current Avenys backend cannot lower this construct yet."
                .to_string(),
        ),
        _ => None,
    }
}

pub type Result<T> = std::result::Result<T, MireError>;

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
    use super::{ErrorKind, MireError};

    #[test]
    fn mire_error_stays_compact_enough_for_result_large_err() {
        let size = std::mem::size_of::<MireError>();
        assert!(
            size <= 80,
            "MireError regressed in size: expected <= 80 bytes, got {size}"
        );

        let err = MireError::new(ErrorKind::Runtime {
            message: "boom".to_string(),
        })
        .with_filename("main.mire".to_string())
        .with_source("use dasu(1)\n".to_string())
        .with_explanation("runtime".to_string());

        assert_eq!(err.filename().map(String::as_str), Some("main.mire"));
        assert!(err.source().is_some());
        assert!(err.explanation().is_some());
    }
}
