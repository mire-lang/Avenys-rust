pub mod mss;

use mss::MssError;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    Lexer {
        line: usize,
        column: usize,
        message: String,
    },
    Parser {
        line: usize,
        column: usize,
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

    pub fn runtime_at(_line: usize, _column: usize, message: String) -> Self {
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

    fn error_type_name(&self) -> &'static str {
        match self {
            ErrorKind::Lexer { .. } => "lexer",
            ErrorKind::Parser { .. } => "parser",
            ErrorKind::Runtime { .. } => "runtime",
            ErrorKind::Type { .. } => "type",
            ErrorKind::Ownership { .. } => "ownership",
        }
    }

    fn error_title(&self) -> &'static str {
        match self {
            ErrorKind::Lexer { .. } => "Lexical Error",
            ErrorKind::Parser { .. } => "Syntax Error",
            ErrorKind::Runtime { .. } => "Runtime Error",
            ErrorKind::Type { .. } => "Type Error",
            ErrorKind::Ownership { .. } => "Ownership Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MireError {
    pub kind: ErrorKind,
    pub source: Option<String>,
    pub filename: Option<String>,
    pub line: usize,
    pub column: usize,
    pub explanation: Option<String>,
}

impl std::fmt::Display for MireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_color())
    }
}

impl std::error::Error for MireError {}

impl MireError {
    pub fn new(kind: ErrorKind) -> Self {
        let (line, column) = match &kind {
            ErrorKind::Lexer { line, column, .. } => (*line, *column),
            ErrorKind::Parser { line, column, .. } => (*line, *column),
            ErrorKind::Runtime { .. } => (1, 1),
            ErrorKind::Type { line, column, .. } => (*line, *column),
            ErrorKind::Ownership { line, column, .. } => (*line, *column),
        };

        let explanation = generate_explanation(&kind);

        Self {
            kind,
            source: None,
            filename: None,
            line,
            column,
            explanation,
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

    pub fn with_explanation(mut self, explanation: String) -> Self {
        self.explanation = Some(explanation);
        self
    }

    pub fn format(&self) -> String {
        self.format_with_options(false)
    }

    pub fn format_color(&self) -> String {
        self.format_with_options(true)
    }

    fn format_with_options(&self, use_color: bool) -> String {
        let message = match &self.kind {
            ErrorKind::Lexer { message, .. } => message.clone(),
            ErrorKind::Parser { message, .. } => message.clone(),
            ErrorKind::Runtime { message } => message.clone(),
            ErrorKind::Type { message, .. } => message.clone(),
            ErrorKind::Ownership { kind, .. } => kind.to_string(),
        };

        let filename = self
            .filename
            .clone()
            .unwrap_or_else(|| "main.mire".to_string());

        let error_type = self.kind.error_type_name();
        let error_title = self.kind.error_title();

        let mut output = String::new();

        if use_color {
            output.push_str(
                "\n\x1b[1;36m✦\x1b[0m \x1b[1;37mAvenys Compiler\x1b[0m \x1b[1;36m✦\x1b[0m\n",
            );
        } else {
            output.push_str("\n✦ Avenys Compiler ✦\n");
        }

        if use_color {
            output.push_str(&format!(
                "\n\x1b[1;31merror[{}]\x1b[0m \x1b[90m───\x1b[0m \x1b[1;33m{}\x1b[0m\n",
                error_type, error_title
            ));
        } else {
            output.push_str(&format!("\nerror[{}] ── {}\n", error_type, error_title));
        }

        if use_color {
            output.push_str(&format!(
                "\x1b[1;36m╭─[ {}:{} ]\x1b[0m\n",
                filename,
                self.format_location()
            ));
        } else {
            output.push_str(&format!(
                "\n╭─[ {}:{} ]\n",
                filename,
                self.format_location()
            ));
        }

        output.push_str(&self.format_code_context(use_color));

        if let Some(explanation) = &self.explanation {
            if use_color {
                output.push_str(&format!(
                    "\x1b[90m╰─\x1b[0m \x1b[90mexplanation:\x1b[0m\n   {}\n",
                    explanation
                ));
            } else {
                output.push_str(&format!("\n╰─ explanation:\n   {}\n", explanation));
            }
        } else {
            if use_color {
                output.push_str("\x1b[90m╰─\x1b[0m\n");
            } else {
                output.push_str("\n╰─\n");
            }
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

            let start_line = if self.line > 2 { self.line - 2 } else { 1 };
            let end_line = (self.line + 2).min(lines.len());

            let line_num_width = end_line.to_string().len();
            let mut output = String::new();

            for (i, line) in lines
                .iter()
                .enumerate()
                .skip(start_line - 1)
                .take(end_line - start_line + 1)
            {
                let line_num = i + 1;

                if line_num == self.line {
                    let error_col = self.column.saturating_sub(1);
                    let marker = if error_col > 0 {
                        format!("{}{}", "─".repeat(error_col), "^^^")
                    } else {
                        "^^^".to_string()
                    };

                    if use_color {
                        output.push_str(&format!(
                            "│ {:width$} │ \x1b[1;37m{}\x1b[0m\n",
                            line_num,
                            line,
                            width = line_num_width
                        ));
                        output.push_str(&format!(
                            "│ {} │ \x1b[1;31m{}\x1b[0m\n",
                            " ".repeat(line_num_width),
                            marker
                        ));
                    } else {
                        output.push_str(&format!(
                            "│ {:width$} │ {}\n",
                            line_num,
                            line,
                            width = line_num_width
                        ));
                        output.push_str(&format!(
                            "│ {} │ {}\n",
                            " ".repeat(line_num_width),
                            marker
                        ));
                    }
                } else {
                    if use_color {
                        output.push_str(&format!(
                            "│ \x1b[1;90m{:width$}\x1b[0m │ \x1b[90m{}\x1b[0m\n",
                            line_num,
                            line,
                            width = line_num_width
                        ));
                    } else {
                        output.push_str(&format!(
                            "│ {:width$} │ {}\n",
                            line_num,
                            line,
                            width = line_num_width
                        ));
                    }
                }
            }

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

pub type Result<T> = std::result::Result<T, MireError>;

fn generate_explanation(kind: &ErrorKind) -> Option<String> {
    match kind {
        ErrorKind::Lexer { message, .. } => {
            if message.contains("Unexpected character") {
                Some("The lexer found a character it cannot process.".to_string())
            } else if message.contains("Unterminated") {
                Some("A string or comment was not properly closed.".to_string())
            } else if message.contains("Invalid") {
                Some("The input contains invalid characters.".to_string())
            } else {
                Some("A lexical error was found while reading the code.".to_string())
            }
        }
        ErrorKind::Parser { message, .. } => {
            if message.contains("Expected") {
                Some("The parser expected different syntax here.".to_string())
            } else if message.contains("Unexpected") {
                Some("The parser found unexpected syntax.".to_string())
            } else if message.contains("Unexpected token") {
                Some("This token is not valid in this context.".to_string())
            } else if message.contains("Unexpected end") {
                Some("The code ended unexpectedly.".to_string())
            } else {
                Some("A syntax error was found.".to_string())
            }
        }
        ErrorKind::Runtime { message } => {
            if message.contains("Undefined") {
                Some("A variable or function was used before being defined.".to_string())
            } else if message.contains("Cannot call") || message.contains("not callable") {
                Some("You tried to call something that is not a function.".to_string())
            } else if message.contains("Index") || message.contains("out of bounds") {
                Some("The index is outside the valid range.".to_string())
            } else if message.contains("division") || message.contains("divide") {
                Some("Cannot divide by zero.".to_string())
            } else if message.contains("No such file") || message.contains("not found") {
                Some("The requested file was not found.".to_string())
            } else {
                Some("An error occurred while running the program.".to_string())
            }
        }
        ErrorKind::Type { message, .. } => {
            if message.contains("mismatch") || message.contains("incompatible") {
                Some("The types do not match what was expected.".to_string())
            } else if message.contains("Unknown identifier") {
                Some("This name has not been defined in the current scope.".to_string())
            } else if message.contains("cannot") && message.contains("+") {
                Some("These types cannot be added together.".to_string())
            } else if message.contains("Expected") {
                Some("The type is different from what was expected.".to_string())
            } else {
                Some("A type error was found.".to_string())
            }
        }
        ErrorKind::Ownership { kind, .. } => Some(format!("Ownership rule violated: {}", kind)),
    }
}

pub fn format_error_chain(errors: &[MireError], _use_color: bool) -> String {
    if errors.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    if errors.len() == 1 {
        output.push_str(&errors[0].format_color());
        return output;
    }

    for (i, error) in errors.iter().enumerate() {
        output.push_str(&error.format_color());
        if i < errors.len() - 1 {
            output.push_str("\n");
        }
    }

    output
}