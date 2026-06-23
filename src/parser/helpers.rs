use crate::error::{ErrorKind, MireError, Result};
use crate::lexer::{Token, TokenType};
use crate::parser::ast::{DataType, Expression, Identifier, Literal};
use std::collections::HashSet;

use super::Parser;

impl Parser {
    pub(super) fn expect_ident(&mut self) -> Result<String> {
        let surface = match self.peek().ttype {
            TokenType::Ident => return Ok(self.advance().value.unwrap_or_default()),
            TokenType::NewKw => "new",
            TokenType::DropKw => "drop",
            TokenType::MoveKw => "move",
            TokenType::OwnKw => "own",
            TokenType::Set => "set",
            TokenType::To => "to",
            _ => return Err(self.error("Expected identifier")),
        };
        self.advance();
        Ok(surface.to_string())
    }

    pub(super) fn expect_member_name(&mut self) -> Result<String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance().value.unwrap_or_default());
        }

        let token = self.peek();
        let surface = self.token_surface(token.clone());
        if is_word_surface(&surface) {
            self.advance();
            return Ok(surface);
        }

        Err(self.error("Expected identifier"))
    }

    pub(super) fn expect_int_literal(&mut self) -> Result<String> {
        if self.check(TokenType::IntLit) {
            Ok(self.advance().value.unwrap_or_default())
        } else {
            Err(self.error("Expected integer literal"))
        }
    }

    pub(super) fn expect_block_close(&mut self) -> Result<()> {
        if self.check(TokenType::Rbrace) || self.is_at_end() {
            self.advance();
            Ok(())
        } else {
            Err(self.error("Expected '}' to close a block"))
        }
    }

    pub(super) fn expect_block_open(&mut self) -> Result<()> {
        if self.check(TokenType::Lbrace) {
            self.advance();
            Ok(())
        } else {
            Err(self.error("Expected '{' to start a block"))
        }
    }

    pub(super) fn check_block_close(&self) -> bool {
        self.check(TokenType::Rbrace)
    }

    pub(super) fn expect(&mut self, token_type: TokenType) -> Result<()> {
        if self.check(token_type) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!(
                "Expected {:?} but found {:?}",
                token_type,
                self.peek().ttype
            )))
        }
    }

    pub(super) fn error_at(&self, line: usize, column: usize, message: &str) -> MireError {
        MireError::new(ErrorKind::Parser {
            line,
            column,
            message: message.to_string(),
        })
    }

    pub(super) fn error(&self, message: &str) -> MireError {
        let token = self.peek();
        self.error_at(token.line, token.column, message)
    }

    pub(super) fn bracket_contains_top_level_comma(&self) -> bool {
        let mut depth = 0usize;
        let mut index = self.pos;
        while let Some(token) = self.tokens.get(index) {
            match token.ttype {
                TokenType::Lbracket | TokenType::Lparen => depth += 1,
                TokenType::Rbracket | TokenType::Rparen => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 && token.ttype == TokenType::Rbracket {
                        return false;
                    }
                }
                TokenType::Comma if depth == 1 => return true,
                _ => {}
            }
            index += 1;
        }
        false
    }

    pub(super) fn is_expression_start(&self, token: TokenType) -> bool {
        matches!(
            token,
            TokenType::Ident
                | TokenType::IntLit
                | TokenType::FloatLit
                | TokenType::CharLit
                | TokenType::StrLit
                | TokenType::BoolLit
                | TokenType::NoneLit
                | TokenType::SelfToken
                | TokenType::Use
                | TokenType::If
                | TokenType::Match
                | TokenType::Lparen
                | TokenType::Lbracket
                | TokenType::Lbrace
                | TokenType::Minus
                | TokenType::Bang
                | TokenType::Amp
                | TokenType::Star
        )
    }

    pub(super) fn is_statement_terminator(&self) -> bool {
        matches!(
            self.peek().ttype,
            TokenType::Newline
                | TokenType::Rbrace
                | TokenType::Else
                | TokenType::Elif
                | TokenType::Eof
        )
    }

    pub(super) fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) {
            self.advance();
        }
    }

    pub(super) fn check(&self, token_type: TokenType) -> bool {
        !self.is_at_end() && self.peek().ttype == token_type
    }

    pub(super) fn peek(&self) -> Token {
        self.tokens
            .get(self.pos)
            .cloned()
            .unwrap_or(Token::new(TokenType::Eof, 0, 0))
    }

    pub(super) fn peek_n(&self, n: usize) -> Token {
        self.tokens
            .get(self.pos + n)
            .cloned()
            .unwrap_or(Token::new(TokenType::Eof, 0, 0))
    }

    pub(super) fn advance(&mut self) -> Token {
        let token = self.peek();
        if !self.is_at_end() {
            self.pos += 1;
        }
        token
    }

    pub(super) fn check_double_colon(&self) -> bool {
        self.check(TokenType::Colon) && self.peek_n(1).ttype == TokenType::Colon
    }

    pub(super) fn is_member_name_token(ttype: TokenType) -> bool {
        matches!(
            ttype,
            TokenType::Ident
                | TokenType::Set
                | TokenType::NewKw
                | TokenType::DropKw
                | TokenType::MoveKw
                | TokenType::OwnKw
        )
    }

    pub(super) fn member_access_name(expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(Identifier { name, .. }) => Some(name.clone()),
            Expression::MemberAccess { target, member, .. } => {
                Self::member_access_name(target).map(|prefix| format!("{}.{}", prefix, member))
            }
            _ => None,
        }
    }

    pub(super) fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.peek().ttype == TokenType::Eof
    }

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn declare(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    pub(super) fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    pub(super) fn push_type_param_scope(&mut self, type_params: Vec<String>) {
        let mut scope = HashSet::new();
        for param in type_params {
            scope.insert(param);
        }
        self.type_param_scopes.push(scope);
    }

    pub(super) fn pop_type_param_scope(&mut self) {
        if self.type_param_scopes.len() > 1 {
            self.type_param_scopes.pop();
        }
    }

    pub(super) fn is_type_param(&self, name: &str) -> bool {
        self.type_param_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }

    pub(super) fn token_surface(&self, token: Token) -> String {
        match token.ttype {
            TokenType::Ident
            | TokenType::IntLit
            | TokenType::FloatLit
            | TokenType::CharLit
            | TokenType::StrLit
            | TokenType::BoolLit => token.value.unwrap_or_default(),
            TokenType::NoneLit => "mu".to_string(),
            TokenType::Colon => ":".to_string(),
            TokenType::Comma => ",".to_string(),
            TokenType::Dot => ".".to_string(),
            TokenType::Plus => "+".to_string(),
            TokenType::Minus => "-".to_string(),
            TokenType::Star => "*".to_string(),
            TokenType::Slash => "/".to_string(),
            TokenType::Percent => "%".to_string(),
            TokenType::Eq => "==".to_string(),
            TokenType::Assign => "=".to_string(),
            TokenType::Neq => "!=".to_string(),
            TokenType::Gt => ">".to_string(),
            TokenType::Lt => "<".to_string(),
            TokenType::Gte => ">=".to_string(),
            TokenType::Lte => "<=".to_string(),
            TokenType::As => "as".to_string(),
            TokenType::At => token.value.unwrap_or_else(|| "at".to_string()),
            TokenType::In => "in".to_string(),
            TokenType::Of => "of".to_string(),
            TokenType::To => "to".to_string(),
            TokenType::NewKw => "new".to_string(),
            TokenType::DropKw => "drop".to_string(),
            TokenType::MoveKw => "move".to_string(),
            TokenType::OwnKw => "own".to_string(),
            TokenType::Question => "?".to_string(),
            TokenType::Lparen => "(".to_string(),
            TokenType::Rparen => ")".to_string(),
            TokenType::Lbracket => "[".to_string(),
            TokenType::Rbracket => "]".to_string(),
            _ => token
                .value
                .unwrap_or_else(|| format!("{:?}", token.ttype).to_lowercase()),
        }
    }

    pub(super) fn coerce_dict_key_to_string(&self, expr: Expression) -> Expression {
        match expr {
            Expression::Identifier(Identifier { name, .. }) if !self.is_declared(&name) => {
                string_expr(&name)
            }
            other => other,
        }
    }
}

pub(super) fn identifier_expr_with_pos(name: &str, line: usize, column: usize) -> Expression {
    Expression::Identifier(Identifier {
        name: name.to_string(),
        data_type: DataType::Unknown,
        line,
        column,
    })
}

pub(super) fn string_expr(value: &str) -> Expression {
    Expression::Literal(Literal::Str(value.to_string()))
}

pub(super) fn data_type_name(data_type: &DataType) -> String {
    match data_type {
        DataType::I8 => "i8".to_string(),
        DataType::I16 => "i16".to_string(),
        DataType::I32 => "i32".to_string(),
        DataType::I64 => "i64".to_string(),
        DataType::U8 => "u8".to_string(),
        DataType::U16 => "u16".to_string(),
        DataType::U32 => "u32".to_string(),
        DataType::U64 => "u64".to_string(),
        DataType::F32 => "f32".to_string(),
        DataType::F64 => "f64".to_string(),
        DataType::Char => "char".to_string(),
        DataType::Str => "str".to_string(),
        DataType::Bool => "bool".to_string(),
        DataType::None => "mu".to_string(),
        DataType::Vector { element_type, .. } => format!("vec[{}]", data_type_name(element_type)),
        DataType::Map {
            key_type,
            value_type,
        } => format!(
            "map[{} {}]",
            data_type_name(key_type),
            data_type_name(value_type)
        ),
        DataType::Array { element_type, size } => {
            format!("arr[{} {}]", data_type_name(element_type), size)
        }
        DataType::Slice { element_type } => format!("slice[{}]", data_type_name(element_type)),
        DataType::Result { ok, err } => {
            format!("result[{} {}]", data_type_name(ok), data_type_name(err))
        }
        DataType::StructNamed(name) | DataType::EnumNamed(name) | DataType::Generic(name) => {
            name.clone()
        }
        _ => "unknown".to_string(),
    }
}

pub(super) fn concat_expressions(mut parts: Vec<Expression>) -> Expression {
    parts.retain(
        |part| !matches!(part, Expression::Literal(Literal::Str(value)) if value.is_empty()),
    );

    if parts.is_empty() {
        return string_expr("");
    }

    if parts.len() == 1 {
        return parts.remove(0);
    }

    let mut expr = parts.remove(0);
    for part in parts {
        expr = Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(expr),
            right: Box::new(part),
            data_type: DataType::Str,
        };
    }
    expr
}

pub(super) fn is_word_surface(surface: &str) -> bool {
    surface
        .chars()
        .next()
        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}

pub fn apply_vector_type_to_list(
    expr: &mut Expression,
    element_type: Box<DataType>,
    dynamic: bool,
) {
    if let Expression::List {
        elements: _,
        element_type: el,
        data_type: dt,
    } = expr
    {
        *el = (*element_type).clone();
        *dt = DataType::Vector {
            element_type,
            dynamic,
        };
    }
}

pub fn apply_map_type_to_dict(
    expr: &mut Expression,
    key_type: Box<DataType>,
    value_type: Box<DataType>,
) {
    if let Expression::Dict {
        entries: _,
        key_type: kt,
        value_type: vt,
        data_type: dt,
    } = expr
    {
        *kt = (*key_type).clone();
        *vt = (*value_type).clone();
        *dt = DataType::Map {
            key_type,
            value_type,
        };
    }
}
