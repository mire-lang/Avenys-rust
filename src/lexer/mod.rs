use crate::error::{ErrorKind, MireError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TokenType {
    Ident,
    IntLit,
    FloatLit,
    StrLit,
    CharLit,
    BoolLit,
    NoneLit,
    Load,
    Module,
    Set,
    Use,
    Return,
    If,
    Elif,
    Else,
    While,
    For,
    Find,
    Do,
    In,
    Fn,
    Type,
    Skill,
    Code,
    Struct,
    Impl,
    Trait,
    Enum,
    Extern,
    Lib,
    Unsafe,
    Asm,
    Extends,
    Match,
    NewKw,
    DropKw,
    MoveKw,
    OwnKw,
    Pub,
    Priv,
    Const,
    Mut,
    As,
    Is,
    Of,
    To,
    At,
    SelfToken,
    Break,
    Continue,
    Eq,
    Assign,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Xor,
    LShift,
    RShift,
    Pipeline,
    PipelineSafe,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    Bang,
    Lparen,
    Rparen,
    Lbracket,
    Rbracket,
    Lbrace,
    Rbrace,
    Colon,
    Comma,
    Dot,
    Question,
    Newline,
    Eof,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenType::Ident => write!(f, "identifier"),
            TokenType::IntLit => write!(f, "integer"),
            TokenType::FloatLit => write!(f, "float"),
            TokenType::StrLit => write!(f, "string"),
            TokenType::CharLit => write!(f, "char"),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub ttype: TokenType,
    pub value: Option<String>,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(ttype: TokenType, line: usize, column: usize) -> Self {
        Self {
            ttype,
            value: None,
            line,
            column,
        }
    }

    pub fn with_value(mut self, value: String) -> Self {
        self.value = Some(value);
        self
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    len: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let len = source.len();
        let token_capacity = (len / 4).max(64);
        Self {
            source,
            pos: 0,
            len,
            line: 1,
            column: 1,
            tokens: Vec::with_capacity(token_capacity),
        }
    }

    fn remaining(&self) -> &'a str {
        &self.source[self.pos..]
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.remaining().chars().nth(offset)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.remaining().chars().next()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek(0) {
            if matches!(c, ' ' | '\t' | '\r') {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) -> Result<bool> {
        if (self.peek(0) == Some('/') && self.peek(1) == Some('/')) || self.peek(0) == Some('#') {
            let is_hash = self.peek(0) == Some('#');
            if is_hash {
                self.advance();
            } else {
                self.advance();
                self.advance();
                if self.peek(0) == Some('!') {
                    self.advance();
                    loop {
                        match (self.peek(0), self.peek(1), self.peek(2)) {
                            (Some('!'), Some('/'), Some('/')) => {
                                self.advance();
                                self.advance();
                                self.advance();
                                return Ok(true);
                            }
                            (None, _, _) => {
                                return Err(MireError::new(ErrorKind::Lexer {
                                    line: self.line,
                                    column: self.column,
                                    message: "Unterminated block comment".to_string(),
                                }));
                            }
                            _ => {
                                self.advance();
                            }
                        }
                    }
                }
            }
            while self.pos < self.len && self.peek(0) != Some('\n') {
                self.advance();
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::with_capacity(16);
        while let Some(c) = self.peek(0) {
            if c.is_alphanumeric() || c == '_' {
                result.push(self.advance().unwrap());
            } else {
                break;
            }
        }
        result
    }

    fn read_number(&mut self) -> String {
        let mut result = String::with_capacity(16);
        let mut has_dot = false;
        while let Some(c) = self.peek(0) {
            if c.is_ascii_digit() {
                result.push(self.advance().unwrap());
            } else if c == '.' && !has_dot && self.peek(1).is_some_and(|n| n.is_ascii_digit()) {
                has_dot = true;
                result.push(self.advance().unwrap());
            } else {
                break;
            }
        }
        result
    }

    fn read_based_integer(&mut self) -> Result<String> {
        let start_line = self.line;
        let start_col = self.column;
        self.advance();
        let base_marker = self.advance().unwrap_or_default();
        let (base, kind) = match base_marker {
            'b' | 'B' => (2, "binary"),
            'o' | 'O' => (8, "octal"),
            'x' | 'X' => (16, "hexadecimal"),
            _ => {
                return Err(MireError::new(ErrorKind::Lexer {
                    line: start_line,
                    column: start_col,
                    message: "Invalid integer base prefix".to_string(),
                }));
            }
        };

        let mut digits = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_ascii_alphanumeric() {
                digits.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        if digits.is_empty() {
            return Err(MireError::new(ErrorKind::Lexer {
                line: start_line,
                column: start_col,
                message: format!("Expected digits after {} prefix", kind),
            }));
        }

        i64::from_str_radix(&digits, base)
            .map(|value| value.to_string())
            .map_err(|_| {
                MireError::new(ErrorKind::Lexer {
                    line: start_line,
                    column: start_col,
                    message: format!("Invalid {} literal '{}'", kind, digits),
                })
            })
    }

    fn read_string(&mut self) -> Result<String> {
        let quote = self.advance().unwrap();
        let mut result = String::new();

        while let Some(c) = self.peek(0) {
            if c == quote {
                self.advance();
                return Ok(result);
            }

            if c == '\\' {
                self.advance();
                match self.peek(0) {
                    Some('n') => {
                        result.push('\n');
                        self.advance();
                    }
                    Some('r') => {
                        result.push('\r');
                        self.advance();
                    }
                    Some('t') => {
                        result.push('\t');
                        self.advance();
                    }
                    Some('\\') => {
                        result.push('\\');
                        self.advance();
                    }
                    Some('"') => {
                        result.push('"');
                        self.advance();
                    }
                    Some('\'') => {
                        result.push('\'');
                        self.advance();
                    }
                    Some('{') => {
                        result.push('{');
                        self.advance();
                    }
                    Some('}') => {
                        result.push('}');
                        self.advance();
                    }
                    Some(other) => {
                        result.push(other);
                        self.advance();
                    }
                    None => break,
                }
                continue;
            }

            if c == '\n' {
                return Err(MireError::new(ErrorKind::Lexer {
                    line: self.line,
                    column: self.column,
                    message: "Unterminated string".to_string(),
                }));
            }

            result.push(self.advance().unwrap());
        }

        Err(MireError::new(ErrorKind::Lexer {
            line: self.line,
            column: self.column,
            message: "Unterminated string".to_string(),
        }))
    }

    fn read_raw_string(&mut self) -> Result<String> {
        let start_line = self.line;
        let start_col = self.column;
        self.advance();

        let mut hashes = 0usize;
        while self.peek(0) == Some('#') {
            hashes += 1;
            self.advance();
        }

        if self.peek(0) != Some('"') {
            return Err(MireError::new(ErrorKind::Lexer {
                line: start_line,
                column: start_col,
                message: "Invalid raw string prefix".to_string(),
            }));
        }
        self.advance();

        let mut result = String::new();
        while let Some(c) = self.peek(0) {
            if c == '"' {
                let mut matches = true;
                for i in 0..hashes {
                    if self.peek(i + 1) != Some('#') {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    self.advance();
                    for _ in 0..hashes {
                        self.advance();
                    }
                    return Ok(result);
                }
            }

            result.push(self.advance().unwrap());
        }

        Err(MireError::new(ErrorKind::Lexer {
            line: start_line,
            column: start_col,
            message: "Unterminated raw string".to_string(),
        }))
    }

    fn read_char_literal(&mut self) -> Result<char> {
        let start_line = self.line;
        let start_col = self.column;
        self.advance();

        let ch = match self.peek(0) {
            Some('\\') => {
                self.advance();
                match self.peek(0) {
                    Some('n') => {
                        self.advance();
                        '\n'
                    }
                    Some('t') => {
                        self.advance();
                        '\t'
                    }
                    Some('r') => {
                        self.advance();
                        '\r'
                    }
                    Some('\\') => {
                        self.advance();
                        '\\'
                    }
                    Some('\'') => {
                        self.advance();
                        '\''
                    }
                    Some('"') => {
                        self.advance();
                        '"'
                    }
                    Some(other) => {
                        return Err(MireError::new(ErrorKind::Lexer {
                            line: start_line,
                            column: start_col,
                            message: format!("Invalid char escape '\\{}'", other),
                        }));
                    }
                    None => {
                        return Err(MireError::new(ErrorKind::Lexer {
                            line: start_line,
                            column: start_col,
                            message: "Unterminated char literal".to_string(),
                        }));
                    }
                }
            }
            Some('\n') | None => {
                return Err(MireError::new(ErrorKind::Lexer {
                    line: start_line,
                    column: start_col,
                    message: "Unterminated char literal".to_string(),
                }));
            }
            Some(value) => {
                self.advance();
                value
            }
        };

        if self.peek(0) != Some('\'') {
            return Err(MireError::new(ErrorKind::Lexer {
                line: start_line,
                column: start_col,
                message: "Char literal must contain exactly one Unicode scalar".to_string(),
            }));
        }
        self.advance();
        Ok(ch)
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>> {
        while self.pos < self.len {
            self.skip_whitespace();

            if self.pos >= self.len {
                break;
            }

            if self.skip_comment()? {
                continue;
            }

            if self.peek(0) == Some('\n') {
                self.advance();
                self.tokens
                    .push(Token::new(TokenType::Newline, self.line, self.column));
                continue;
            }

            let c = match self.peek(0) {
                Some(c) => c,
                None => break,
            };

            if c == 'r' && (self.peek(1) == Some('"') || self.peek(1) == Some('#')) {
                let value = self.read_raw_string()?;
                self.tokens
                    .push(Token::new(TokenType::StrLit, self.line, self.column).with_value(value));
                continue;
            }

            if c.is_alphabetic() || c == '_' {
                let start_line = self.line;
                let start_col = self.column;
                let ident = self.read_identifier();
                let token = match ident.as_str() {
                    "set" => Token::new(TokenType::Set, self.line, self.column),
                    "load" => Token::new(TokenType::Load, self.line, self.column),
                    "module" => Token::new(TokenType::Module, self.line, self.column),
                    "use" => Token::new(TokenType::Use, self.line, self.column),
                    "return" => Token::new(TokenType::Return, self.line, self.column),
                    "if" => Token::new(TokenType::If, self.line, self.column),
                    "elif" => Token::new(TokenType::Elif, self.line, self.column),
                    "else" => Token::new(TokenType::Else, self.line, self.column),
                    "while" => Token::new(TokenType::While, self.line, self.column),
                    "for" => Token::new(TokenType::For, self.line, self.column),
                    "find" => Token::new(TokenType::Find, self.line, self.column),
                    "do" => Token::new(TokenType::Do, self.line, self.column),
                    "in" => Token::new(TokenType::In, self.line, self.column),
                    "fn" => Token::new(TokenType::Fn, self.line, self.column),
                    "type" => Token::new(TokenType::Type, self.line, self.column),
                    "skill" => Token::new(TokenType::Skill, self.line, self.column),
                    "struct" => Token::new(TokenType::Struct, self.line, self.column),
                    "impl" => Token::new(TokenType::Impl, self.line, self.column),
                    "enum" => Token::new(TokenType::Enum, self.line, self.column),
                    "extern" => Token::new(TokenType::Extern, self.line, self.column),
                    "lib" => Token::new(TokenType::Lib, self.line, self.column),
                    "unsafe" => Token::new(TokenType::Unsafe, self.line, self.column),
                    "asm" => Token::new(TokenType::Asm, self.line, self.column),
                    "extends" => Token::new(TokenType::Extends, self.line, self.column),
                    "mu" => Token::new(TokenType::NoneLit, self.line, self.column),
                    "match" => Token::new(TokenType::Match, self.line, self.column),
                    "new" => Token::new(TokenType::NewKw, self.line, self.column),
                    "drop" => Token::new(TokenType::DropKw, self.line, self.column),
                    "move" => Token::new(TokenType::MoveKw, self.line, self.column),
                    "own" => Token::new(TokenType::OwnKw, self.line, self.column),
                    "pub" => Token::new(TokenType::Pub, self.line, self.column),
                    "priv" => Token::new(TokenType::Priv, self.line, self.column),
                    "const" => Token::new(TokenType::Const, self.line, self.column),
                    "mut" => Token::new(TokenType::Mut, self.line, self.column),
                    "as" => Token::new(TokenType::As, self.line, self.column),
                    "is" => Token::new(TokenType::Is, self.line, self.column),
                    "of" => Token::new(TokenType::Of, self.line, self.column),
                    "to" => Token::new(TokenType::To, self.line, self.column),
                    "at" => Token::new(TokenType::At, self.line, self.column),
                    "self" => Token::new(TokenType::SelfToken, start_line, start_col),
                    "break" => Token::new(TokenType::Break, start_line, start_col),
                    "continue" => Token::new(TokenType::Continue, start_line, start_col),
                    "true" | "false" => {
                        Token::new(TokenType::BoolLit, start_line, start_col).with_value(ident)
                    }
                    _ => Token::new(TokenType::Ident, start_line, start_col).with_value(ident),
                };
                self.tokens.push(token);
                continue;
            }

            if c.is_ascii_digit() {
                let num = if c == '0'
                    && matches!(self.peek(1), Some('b' | 'B' | 'o' | 'O' | 'x' | 'X'))
                {
                    self.read_based_integer()?
                } else {
                    self.read_number()
                };
                let token = if num.contains('.') {
                    Token::new(TokenType::FloatLit, self.line, self.column - num.len())
                } else {
                    Token::new(TokenType::IntLit, self.line, self.column - num.len())
                }
                .with_value(num);
                self.tokens.push(token);
                continue;
            }

            if c == '"' {
                let value = self.read_string()?;
                self.tokens
                    .push(Token::new(TokenType::StrLit, self.line, self.column).with_value(value));
                continue;
            }

            if c == '\'' {
                let value = self.read_char_literal()?;
                self.tokens.push(
                    Token::new(TokenType::CharLit, self.line, self.column)
                        .with_value((value as u32).to_string()),
                );
                continue;
            }

            let start_line = self.line;
            let start_col = self.column;
            let token = match c {
                '=' => {
                    self.advance();
                    if self.peek(0) == Some('>') {
                        self.advance();
                        if self.peek(0) == Some('?') {
                            self.advance();
                            Token::new(TokenType::PipelineSafe, start_line, start_col)
                        } else {
                            Token::new(TokenType::Pipeline, start_line, start_col)
                        }
                    } else if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Eq, start_line, start_col)
                    } else {
                        Token::new(TokenType::Assign, start_line, start_col)
                    }
                }
                '+' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::PlusAssign, start_line, start_col)
                    } else {
                        Token::new(TokenType::Plus, start_line, start_col)
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::MinusAssign, start_line, start_col)
                    } else {
                        Token::new(TokenType::Minus, start_line, start_col)
                    }
                }
                '*' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::StarAssign, start_line, start_col)
                    } else {
                        Token::new(TokenType::Star, start_line, start_col)
                    }
                }
                '/' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::SlashAssign, start_line, start_col)
                    } else {
                        Token::new(TokenType::Slash, start_line, start_col)
                    }
                }
                '%' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::PercentAssign, start_line, start_col)
                    } else {
                        Token::new(TokenType::Percent, start_line, start_col)
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Neq, start_line, start_col)
                    } else {
                        Token::new(TokenType::Bang, start_line, start_col)
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Gte, start_line, start_col)
                    } else if self.peek(0) == Some('>') {
                        self.advance();
                        Token::new(TokenType::RShift, start_line, start_col)
                    } else {
                        Token::new(TokenType::Gt, start_line, start_col)
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Lte, start_line, start_col)
                    } else if self.peek(0) == Some('<') {
                        self.advance();
                        Token::new(TokenType::LShift, start_line, start_col)
                    } else {
                        Token::new(TokenType::Lt, start_line, start_col)
                    }
                }
                '&' => {
                    self.advance();
                    if self.peek(0) == Some('&') {
                        self.advance();
                        Token::new(TokenType::AmpAmp, start_line, start_col)
                    } else {
                        Token::new(TokenType::Amp, start_line, start_col)
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek(0) == Some('|') {
                        self.advance();
                        Token::new(TokenType::PipePipe, start_line, start_col)
                    } else {
                        Token::new(TokenType::Pipe, start_line, start_col)
                    }
                }
                '^' => {
                    self.advance();
                    Token::new(TokenType::Xor, start_line, start_col)
                }
                '(' => {
                    self.advance();
                    Token::new(TokenType::Lparen, start_line, start_col)
                }
                ')' => {
                    self.advance();
                    Token::new(TokenType::Rparen, start_line, start_col)
                }
                '[' => {
                    self.advance();
                    Token::new(TokenType::Lbracket, start_line, start_col)
                }
                ']' => {
                    self.advance();
                    Token::new(TokenType::Rbracket, start_line, start_col)
                }
                '{' => {
                    self.advance();
                    Token::new(TokenType::Lbrace, start_line, start_col)
                }
                '}' => {
                    self.advance();
                    Token::new(TokenType::Rbrace, start_line, start_col)
                }
                ':' => {
                    self.advance();
                    Token::new(TokenType::Colon, start_line, start_col)
                }
                ',' => {
                    self.advance();
                    Token::new(TokenType::Comma, start_line, start_col)
                }
                '.' => {
                    self.advance();
                    Token::new(TokenType::Dot, start_line, start_col)
                }
                '@' => {
                    self.advance();
                    Token::new(TokenType::At, start_line, start_col).with_value("@".to_string())
                }
                '?' => {
                    self.advance();
                    Token::new(TokenType::Question, start_line, start_col)
                }
                _ => {
                    return Err(MireError::new(ErrorKind::Lexer {
                        line: self.line,
                        column: self.column,
                        message: format!("Unexpected character '{}'", c),
                    }));
                }
            };
            self.tokens.push(token);
        }

        self.tokens
            .push(Token::new(TokenType::Eof, self.line, self.column));
        Ok(self.tokens)
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Token>> {
    Lexer::new(source).tokenize()
}
