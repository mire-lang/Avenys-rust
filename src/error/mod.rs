pub mod mss;

use mss::MssError;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ErrorKind {
    #[error("Lexical error")]
    Lexer {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Syntax error")]
    Parser {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Runtime error")]
    Runtime { message: String },

    #[error("Type error")]
    Type { message: String },

    #[error("Ownership error")]
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

    pub fn runtime_at(_line: usize, _column: usize, message: String) -> Self {
        ErrorKind::Runtime { message }
    }

    pub fn type_error(message: String) -> Self {
        ErrorKind::Type { message }
    }

    pub fn type_error_at(_line: usize, _column: usize, message: String) -> Self {
        ErrorKind::Type { message }
    }

    pub fn ownership_error(line: usize, column: usize, kind: MssError) -> Self {
        ErrorKind::Ownership { line, column, kind }
    }
}

#[derive(Debug, Clone)]
pub struct MireError {
    pub kind: ErrorKind,
    pub source: Option<String>,
    pub filename: Option<String>,
    pub line: usize,
    pub column: usize,
    pub hint: Option<String>,
    pub suggestion: Option<String>,
}

impl std::fmt::Display for MireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_color())
    }
}

impl std::error::Error for MireError {}

impl MireError {
    pub fn new(kind: ErrorKind) -> Self {
        let (line, column, hint) = match &kind {
            ErrorKind::Lexer { line, column, .. } => (*line, *column, None),
            ErrorKind::Parser { line, column, .. } => (*line, *column, None),
            ErrorKind::Runtime { message } => (1, 1, runtime_hint(message)),
            ErrorKind::Type { .. } => (1, 1, None),
            ErrorKind::Ownership { line, column, kind } => (*line, *column, Some(kind.to_string())),
        };
        Self {
            kind,
            source: None,
            filename: None,
            line,
            column,
            hint,
            suggestion: None,
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_filename(mut self, filename: String) -> Self {
        self.filename = Some(filename);
        self
    }

    pub fn with_hint(mut self, hint: String) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.suggestion = Some(suggestion);
        self
    }

    pub fn format(&self) -> String {
        self.format_with_options(false, true)
    }

    pub fn format_color(&self) -> String {
        self.format_with_options(true, true)
    }

    pub fn format_simple(&self) -> String {
        self.format_with_options(false, false)
    }

    fn format_with_options(&self, use_color: bool, show_context: bool) -> String {
        let message = match &self.kind {
            ErrorKind::Lexer { message, .. } => message.clone(),
            ErrorKind::Parser { message, .. } => message.clone(),
            ErrorKind::Runtime { message } => message.clone(),
            ErrorKind::Type { message } => message.clone(),
            ErrorKind::Ownership { kind, .. } => kind.to_string(),
        };

        let filename = self
            .filename
            .clone()
            .unwrap_or_else(|| "main.mire".to_string());

        let error_type_str: &str = match &self.kind {
            ErrorKind::Lexer { .. } => "Lexer",
            ErrorKind::Parser { .. } => "Parser",
            ErrorKind::Runtime { .. } => "Runtime",
            ErrorKind::Type { .. } => "Type",
            ErrorKind::Ownership { .. } => "Ownership",
        };

        let header = if use_color {
            format!(
                "\x1b[1;31merror\x1b[0m[{}]: {}",
                error_type_str.to_lowercase(),
                message
            )
        } else {
            format!("error[{}]: {}", error_type_str.to_lowercase(), message)
        };

        let location_str = if use_color {
            format!(
                "\x1b[1;34m--> {}:{}\x1b[0m",
                filename,
                self.format_location()
            )
        } else {
            format!("--> {}:{}", filename, self.format_location())
        };

        let mut output = format!("{}\n{}\n", location_str, header);

        if show_context {
            output.push_str(&self.format_code_context(use_color));
        }

        if let Some(hint) = &self.hint {
            let hint_str = if use_color {
                format!("\x1b[1;90mhelp\x1b[0m: {}", hint)
            } else {
                format!("help: {}", hint)
            };
            output.push_str(&format!("{}\n", hint_str));
        }

        if let Some(suggestion) = &self.suggestion {
            let sugg_str = if use_color {
                format!("\x1b[1;32msuggestion\x1b[0m: {}", suggestion)
            } else {
                format!("suggestion: {}", suggestion)
            };
            output.push_str(&format!("{}\n", sugg_str));
        }

        output
    }

    fn format_location(&self) -> String {
        format!("{}:{}", self.line, self.column)
    }

    fn format_code_context(&self, use_color: bool) -> String {
        if let Some(source) = &self.source {
            let lines: Vec<&str> = source.lines().collect();
            if self.line == 0 || self.line > lines.len() {
                return String::new();
            }

            let start_line = self.line.saturating_sub(2);
            let end_line = (self.line + 2).min(lines.len());

            let line_num_width = end_line.to_string().len();
            let mut output = String::new();

            for (i, line) in lines
                .iter()
                .enumerate()
                .skip(start_line)
                .take(end_line - start_line)
            {
                let line_num = i + 1;
                let line_num_str = format!("{:width$}", line_num, width = line_num_width);

                if line_num == self.line {
                    let caret_pos = self.column.saturating_sub(1);
                    let line_len = line.len();
                    let caret_len = line_len.saturating_sub(caret_pos).max(1);

                    if use_color {
                        output
                            .push_str(&format!("\x1b[1;32m{:>}\x1b[0m |{}\n", line_num_str, line));
                        output.push_str(&format!(
                            "  {} |{}{}\x1b[31m{}\x1b[0m",
                            " ".repeat(line_num_width),
                            " ".repeat(caret_pos),
                            "^".repeat(caret_len.min(3).max(1)),
                            if caret_len > 3 { "..." } else { "" }
                        ));
                    } else {
                        output.push_str(&format!("{:>}|{}\n", line_num_str, line));
                        output.push_str(&format!(
                            "  {}{}{}",
                            " ".repeat(line_num_width),
                            " ".repeat(caret_pos),
                            "^".repeat(caret_len.min(3).max(1))
                        ));
                    }
                } else {
                    if use_color {
                        output.push_str(&format!(
                            "\x1b[1;90m{:>}\x1b[0m |\x1b[90m{}\x1b[0m\n",
                            line_num_str, line
                        ));
                    } else {
                        output.push_str(&format!("{:>}|{}\n", line_num_str, line));
                    }
                }
            }

            output.push('\n');
            output
        } else {
            String::new()
        }
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
    pub fn runtime(message: String) -> Self {
        Self::new(ErrorKind::Runtime { message })
    }

    pub fn runtime_at(line: usize, column: usize, message: String) -> Self {
        let mut error = Self::new(ErrorKind::Runtime { message });
        error.line = line;
        error.column = column;
        error
    }

    pub fn type_error(message: String) -> Self {
        Self::new(ErrorKind::Type { message })
    }

    pub fn type_error_at(line: usize, column: usize, message: String) -> Self {
        let mut error = Self::new(ErrorKind::Type { message });
        error.line = line;
        error.column = column;
        error
    }

    pub fn ownership_error(line: usize, column: usize, kind: MssError) -> Self {
        Self::new(ErrorKind::Ownership { line, column, kind })
    }
}

pub type Result<T> = std::result::Result<T, MireError>;

fn runtime_hint(message: &str) -> Option<String> {
    if message.starts_with("Undefined variable: ") {
        Some("Declare the variable with add <type> > (name = ...), or check scope.".to_string())
    } else if message.starts_with("Undefined field: ")
        || message.starts_with("Unknown field ")
        || message.starts_with("Field '")
    {
        Some("Check the field name, visibility, and that the instance has the field.".to_string())
    } else if message.contains("Cannot access member on class") {
        Some("Instantiate with ClassName(...) before accessing members.".to_string())
    } else if message.contains("Cannot assign to '") && message.contains("already borrowed") {
        Some("Release references (drop) or re-assign the reference before mutating.".to_string())
    } else if message.contains("Cannot borrow '") {
        Some("Avoid mixing mutable and immutable references at the same time.".to_string())
    } else if message.contains("Cannot index") {
        Some("Ensure the value is list/tuple/dict and the index is in range.".to_string())
    } else if message.contains("Index out of bounds") {
        Some("Check index bounds and collection length.".to_string())
    } else if message.contains("is not callable") || message.contains("Cannot call") {
        Some("Ensure the value is a function/method before calling.".to_string())
    } else {
        None
    }
}

pub fn format_error_chain(errors: &[MireError], use_color: bool) -> String {
    if errors.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    if errors.len() == 1 {
        if use_color {
            output.push_str(&errors[0].format_color());
        } else {
            output.push_str(&errors[0].format());
        }
        return output;
    }

    for (i, error) in errors.iter().enumerate() {
        if use_color {
            output.push_str(&format!("\x1b[1;31merror[{}]\x1b[0m\n", i + 1));
            output.push_str(&error.format_color());
        } else {
            output.push_str(&format!("error[{}]\n", i + 1));
            output.push_str(&error.format());
        }
        if i < errors.len() - 1 {
            output.push_str("\n");
        }
    }

    output
}
