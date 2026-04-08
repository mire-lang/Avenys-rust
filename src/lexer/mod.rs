use crate::error::{ErrorKind, MireError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TokenType {
    Ident,
    IntLit,
    FloatLit,
    StrLit,
    BoolLit,
    NoneLit,
    Add,
    Set,
    Use,
    Return,
    If,
    Elif,
    Else,
    While,
    For,
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
    Extends,
    Match,
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
    And,
    Or,
    Not,
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
    Pipeline,
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

pub struct Lexer {
    source_chars: Vec<char>,
    pos: usize,
    len: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

impl Lexer {
    pub fn new(source: String) -> Self {
        let source_chars: Vec<char> = source.chars().collect();
        let len = source_chars.len();
        Self {
            source_chars,
            pos: 0,
            len,
            line: 1,
            column: 1,
            tokens: Vec::new(),
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.source_chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = *self.source_chars.get(self.pos)?;
        self.pos += 1;
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
        if self.peek(0) == Some('\\') && self.peek(1) == Some('!') {
            self.advance();
            self.advance();
            while self.pos < self.len {
                if self.peek(0) == Some('!') && self.peek(1) == Some('\\') {
                    self.advance();
                    self.advance();
                    return Ok(true);
                }
                self.advance();
            }
            return Err(MireError::new(ErrorKind::Lexer {
                line: self.line,
                column: self.column,
                message: "Unterminated comment".to_string(),
            }));
        }
        Ok(false)
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::new();
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
        let mut result = String::new();
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

            if c.is_alphabetic() || c == '_' {
                let ident = self.read_identifier();
                let token = match ident.as_str() {
                    "add" => Token::new(TokenType::Add, self.line, self.column),
                    "set" => Token::new(TokenType::Set, self.line, self.column),
                    "use" => Token::new(TokenType::Use, self.line, self.column),
                    "return" => Token::new(TokenType::Return, self.line, self.column),
                    "if" => Token::new(TokenType::If, self.line, self.column),
                    "elif" => Token::new(TokenType::Elif, self.line, self.column),
                    "else" => Token::new(TokenType::Else, self.line, self.column),
                    "while" => Token::new(TokenType::While, self.line, self.column),
                    "for" => Token::new(TokenType::For, self.line, self.column),
                    "do" => Token::new(TokenType::Do, self.line, self.column),
                    "in" => Token::new(TokenType::In, self.line, self.column),
                    "fn" => Token::new(TokenType::Fn, self.line, self.column),
                    "type" => Token::new(TokenType::Type, self.line, self.column),
                    "skill" => Token::new(TokenType::Skill, self.line, self.column),
                    "code" => Token::new(TokenType::Code, self.line, self.column),
                    "struct" => Token::new(TokenType::Struct, self.line, self.column),
                    "impl" => Token::new(TokenType::Impl, self.line, self.column),
                    "trait" => Token::new(TokenType::Trait, self.line, self.column),
                    "enum" => Token::new(TokenType::Enum, self.line, self.column),
                    "extends" => Token::new(TokenType::Extends, self.line, self.column),
                    "mu" => Token::new(TokenType::NoneLit, self.line, self.column),
                    "match" => Token::new(TokenType::Match, self.line, self.column),
                    "pub" => Token::new(TokenType::Pub, self.line, self.column),
                    "priv" => Token::new(TokenType::Priv, self.line, self.column),
                    "const" => Token::new(TokenType::Const, self.line, self.column),
                    "mut" => Token::new(TokenType::Mut, self.line, self.column),
                    "as" => Token::new(TokenType::As, self.line, self.column),
                    "is" => Token::new(TokenType::Is, self.line, self.column),
                    "of" => Token::new(TokenType::Of, self.line, self.column),
                    "to" => Token::new(TokenType::To, self.line, self.column),
                    "at" => Token::new(TokenType::At, self.line, self.column),
                    "self" => Token::new(TokenType::SelfToken, self.line, self.column),
                    "and" => Token::new(TokenType::And, self.line, self.column),
                    "or" => Token::new(TokenType::Or, self.line, self.column),
                    "not" => Token::new(TokenType::Not, self.line, self.column),
                    "break" => Token::new(TokenType::Break, self.line, self.column),
                    "continue" => Token::new(TokenType::Continue, self.line, self.column),
                    "true" | "false" => {
                        Token::new(TokenType::BoolLit, self.line, self.column).with_value(ident)
                    }
                    "none" => Token::new(TokenType::NoneLit, self.line, self.column),
                    _ => Token::new(TokenType::Ident, self.line, self.column).with_value(ident),
                };
                self.tokens.push(token);
                continue;
            }

            if c.is_ascii_digit() {
                let num = self.read_number();
                let token = if num.contains('.') {
                    Token::new(TokenType::FloatLit, self.line, self.column - num.len())
                } else {
                    Token::new(TokenType::IntLit, self.line, self.column - num.len())
                }
                .with_value(num);
                self.tokens.push(token);
                continue;
            }

            if c == '"' || c == '\'' {
                let value = self.read_string()?;
                self.tokens
                    .push(Token::new(TokenType::StrLit, self.line, self.column).with_value(value));
                continue;
            }

            let token = match c {
                '=' => {
                    self.advance();
                    if self.peek(0) == Some('>') {
                        self.advance();
                        Token::new(TokenType::Pipeline, self.line, self.column)
                    } else if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Eq, self.line, self.column)
                    } else {
                        Token::new(TokenType::Assign, self.line, self.column)
                    }
                }
                '+' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::PlusAssign, self.line, self.column)
                    } else {
                        Token::new(TokenType::Plus, self.line, self.column)
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::MinusAssign, self.line, self.column)
                    } else {
                        Token::new(TokenType::Minus, self.line, self.column)
                    }
                }
                '*' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::StarAssign, self.line, self.column)
                    } else {
                        Token::new(TokenType::Star, self.line, self.column)
                    }
                }
                '/' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::SlashAssign, self.line, self.column)
                    } else {
                        Token::new(TokenType::Slash, self.line, self.column)
                    }
                }
                '%' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::PercentAssign, self.line, self.column)
                    } else {
                        Token::new(TokenType::Percent, self.line, self.column)
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Neq, self.line, self.column)
                    } else {
                        Token::new(TokenType::Bang, self.line, self.column)
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Gte, self.line, self.column)
                    } else {
                        Token::new(TokenType::Gt, self.line, self.column)
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek(0) == Some('=') {
                        self.advance();
                        Token::new(TokenType::Lte, self.line, self.column)
                    } else {
                        Token::new(TokenType::Lt, self.line, self.column)
                    }
                }
                '&' => {
                    self.advance();
                    Token::new(TokenType::Amp, self.line, self.column)
                }
                '(' => {
                    self.advance();
                    Token::new(TokenType::Lparen, self.line, self.column)
                }
                ')' => {
                    self.advance();
                    Token::new(TokenType::Rparen, self.line, self.column)
                }
                '[' => {
                    self.advance();
                    Token::new(TokenType::Lbracket, self.line, self.column)
                }
                ']' => {
                    self.advance();
                    Token::new(TokenType::Rbracket, self.line, self.column)
                }
                '{' => {
                    self.advance();
                    Token::new(TokenType::Lbrace, self.line, self.column)
                }
                '}' => {
                    self.advance();
                    Token::new(TokenType::Rbrace, self.line, self.column)
                }
                ':' => {
                    self.advance();
                    Token::new(TokenType::Colon, self.line, self.column)
                }
                ',' => {
                    self.advance();
                    Token::new(TokenType::Comma, self.line, self.column)
                }
                '.' => {
                    self.advance();
                    Token::new(TokenType::Dot, self.line, self.column)
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
    Lexer::new(source.to_string()).tokenize()
}
