use super::{DiagnosticCode, ErrorKind, Span};

pub(crate) fn map_kind(kind: &ErrorKind) -> (Span, &'static str, String, DiagnosticCode) {
    match kind {
        ErrorKind::Lexer { span, message } => (*span, "Lexical Error", message.clone(), DiagnosticCode::E0001),
        ErrorKind::DeprecatedSyntax { span, message } => (
            *span,
            "Deprecated Syntax",
            message.clone(),
            DiagnosticCode::W0010,
        ),
        ErrorKind::Parser { span, message } => (*span, "Syntax Error", message.clone(), DiagnosticCode::E0003),
        ErrorKind::Backend { span, message } => (
            *span,
            "Backend Limitation",
            message.clone(),
            DiagnosticCode::E0014,
        ),
        ErrorKind::Runtime { span, message } => (*span, "Runtime Error", message.clone(), DiagnosticCode::E0015),
        ErrorKind::Type { span, message, code } => (
            *span,
            "Type Error",
            message.clone(),
            code.unwrap_or(DiagnosticCode::E0005),
        ),
        ErrorKind::Ownership { span, kind } => (*span, "Ownership Error", kind.to_string(), kind.diagnostic_code()),
        ErrorKind::Cli { message } => (Span::unknown(), "CLI Error", message.clone(), DiagnosticCode::E0017),
    }
}

pub(crate) fn default_help_for_code(code: DiagnosticCode) -> Option<String> {
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
