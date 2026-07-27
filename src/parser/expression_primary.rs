use crate::error::Result;
use crate::lexer::TokenType;
use crate::parser::ast::{DataType, Expression, Literal, Statement};

use super::Parser;
use super::helpers::{data_type_name, identifier_expr_with_pos};

impl Parser {
    pub(super) fn bracket_followed_by_lparen(&self) -> bool {
        if !self.check(TokenType::Lbracket) {
            return false;
        }
        let mut depth = 0usize;
        let mut idx = self.pos;
        while let Some(tok) = self.tokens.get(idx) {
            match tok.ttype {
                TokenType::Lbracket => depth += 1,
                TokenType::Rbracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let next = self
                            .tokens
                            .get(idx + 1)
                            .map(|t| t.ttype)
                            .unwrap_or(TokenType::Eof);
                        return next == TokenType::Lparen;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(super) fn bracket_followed_by_dot(&self) -> bool {
        if !self.check(TokenType::Lbracket) {
            return false;
        }
        let mut depth = 0usize;
        let mut idx = self.pos;
        while let Some(tok) = self.tokens.get(idx) {
            match tok.ttype {
                TokenType::Lbracket => depth += 1,
                TokenType::Rbracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let next = self
                            .tokens
                            .get(idx + 1)
                            .map(|t| t.ttype)
                            .unwrap_or(TokenType::Eof);
                        return next == TokenType::Dot;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(super) fn bracket_followed_by_ident_colon(&self) -> bool {
        if !self.check(TokenType::Lbracket) {
            return false;
        }
        let mut depth = 0usize;
        let mut idx = self.pos;
        while let Some(tok) = self.tokens.get(idx) {
            match tok.ttype {
                TokenType::Lbracket => depth += 1,
                TokenType::Rbracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let next = self
                            .tokens
                            .get(idx + 1)
                            .map(|t| t.ttype)
                            .unwrap_or(TokenType::Eof);
                        let next2 = self
                            .tokens
                            .get(idx + 2)
                            .map(|t| t.ttype)
                            .unwrap_or(TokenType::Eof);
                        return next == TokenType::Ident && next2 == TokenType::Colon;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(super) fn parse_primary(&mut self) -> Result<Expression> {
        if self.check_lifecycle_expression_prefix() {
            return self.parse_lifecycle_expression();
        }

        if self.check_keyword_ident() {
            return Ok(self.parse_keyword_identifier());
        }

        match self.peek().ttype {
            TokenType::Use => self.parse_use_expr(),
            TokenType::If => self.parse_if_expression(),
            TokenType::Match => {
                self.advance();
                self.parse_match_expression()
            }
            TokenType::IntLit => {
                let token = self.advance();
                let value = token.value.unwrap_or_default();
                let parsed = value.parse().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid integer literal '{}'", value),
                    )
                })?;
                Ok(Expression::Literal { lit: Literal::Int(parsed), line: token.line, column: token.column })
            }
            TokenType::FloatLit => {
                let token = self.advance();
                let value = token.value.unwrap_or_default();
                let parsed = value.parse().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid float literal '{}'", value),
                    )
                })?;
                Ok(Expression::Literal { lit: Literal::Float(parsed), line: token.line, column: token.column })
            }
            TokenType::CharLit => {
                let token = self.advance();
                let value = token.value.unwrap_or_default();
                let parsed = value.parse::<u32>().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid char literal '{}'", value),
                    )
                })?;
                Ok(Expression::Literal { lit: Literal::Char(parsed), line: token.line, column: token.column })
            }
            TokenType::StrLit => {
                let token = self.advance();
                let value = token.value.unwrap_or_default();
                Ok(Expression::Literal { lit: Literal::Str(value), line: token.line, column: token.column })
            }
            TokenType::BoolLit => {
                let token = self.advance();
                let value = token.value.unwrap_or_default();
                Ok(Expression::Literal { lit: Literal::Bool(value == "true"), line: token.line, column: token.column })
            }
            TokenType::NoneLit => {
                let token = self.advance();
                Ok(Expression::Literal { lit: Literal::None, line: token.line, column: token.column })
            }
            TokenType::SelfToken => {
                let token = self.peek();
                self.advance();
                Ok(identifier_expr_with_pos("self", token.line, token.column))
            }
            TokenType::Ident => {
                let token = self.peek();
                let base_name = self.advance().value.unwrap_or_default();
                let name = if self.check(TokenType::Lbracket) && self.bracket_followed_by_dot() {
                    let type_args = self.parse_type_args()?;
                    format!(
                        "{}[{}]",
                        base_name,
                        type_args
                            .iter()
                            .map(data_type_name)
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                } else {
                    base_name
                };
                if name == "type" && self.is_expression_start(self.peek().ttype) {
                    let expr = self.parse_expression()?;
                    return Ok(Expression::Call {
                        name: "type".to_string(),
                        args: vec![expr],
                        type_args: Vec::new(),
                        name_line: 0,
            name_column: 0,
            data_type: DataType::Str,
                    });
                }
                if self.check_double_colon() && Self::is_member_name_token(self.peek_n(2).ttype) {
                    let mut full_name = name.clone();
                    while self.check(TokenType::Colon) && self.peek_n(1).ttype == TokenType::Colon && Self::is_member_name_token(self.peek_n(2).ttype) {
                        self.advance(); // first :
                        self.advance(); // second :
                        full_name.push('.');
                        full_name.push_str(&self.expect_member_name()?);
                    }

                    if self.check(TokenType::Dot) && self.peek_n(1).ttype == TokenType::Ident {
                        let last_part = full_name.split('.').next_back().unwrap_or("");
                        if self.enum_names.contains(last_part) {
                            self.advance();
                            let variant_name = self.advance().value.unwrap_or_default();
                            if self.check(TokenType::Lparen) {
                                let payloads = self.parse_enum_variant_arguments()?;
                                return Ok(Expression::EnumVariant {
                                    enum_name: full_name.clone(),
                                    variant_name,
                                    payloads,
                                    data_type: DataType::EnumNamed(full_name),
                                    line: token.line,
                                    column: token.column,
                                });
                            }
                            return Ok(Expression::EnumVariantPath {
                                enum_name: full_name.clone(),
                                variant_name,
                                data_type: DataType::EnumNamed(full_name),
                                line: token.line,
                                column: token.column,
                            });
                        }
                    }

                    if self.check(TokenType::Lparen) {
                        let args = self.parse_call_arguments()?;
                        return Ok(Expression::Call {
                            name: full_name,
                            args,
                            type_args: Vec::new(),
                            name_line: token.line,
                            name_column: token.column,
                            data_type: DataType::Unknown,
                        });
                    }

                    return Ok(Expression::MemberAccess {
                        target: Box::new(identifier_expr_with_pos(&name, token.line, token.column)),
                        member: full_name.strip_prefix(&format!("{}.", name)).unwrap_or(&full_name).to_string(),
                        data_type: DataType::Unknown,
                    });
                }

                if self.check(TokenType::Dot) && self.peek_n(1).ttype == TokenType::Ident {
                    if self
                        .enum_names
                        .contains(name.split('[').next().unwrap_or(&name))
                    {
                        self.advance();
                        let variant_name = self.advance().value.unwrap_or_default();
                        if self.check(TokenType::Lparen) {
                            let payloads = self.parse_enum_variant_arguments()?;
                            let enum_name = name;
                            return Ok(Expression::EnumVariant {
                                enum_name: enum_name.clone(),
                                variant_name,
                                payloads,
                                data_type: DataType::EnumNamed(enum_name),
                                line: token.line,
                                column: token.column,
                            });
                        }
                        let enum_name = name;
                        return Ok(Expression::EnumVariantPath {
                            enum_name: enum_name.clone(),
                            variant_name,
                            data_type: DataType::EnumNamed(enum_name),
                            line: token.line,
                            column: token.column,
                        });
                    }
                    self.advance();
                    let member = self.expect_member_name()?;
                    if self.check(TokenType::Lparen) {
                        let full_name = format!("{}.{}", name, member);
                        let args = self.parse_call_arguments()?;
                        return Ok(Expression::Call {
                            name: full_name,
                            args,
                            type_args: Vec::new(),
                            name_line: token.line,
                            name_column: token.column,
                            data_type: DataType::Unknown,
                        });
                    }
                    return Ok(Expression::MemberAccess {
                        target: Box::new(identifier_expr_with_pos(&name, token.line, token.column)),
                        member,
                        data_type: DataType::Unknown,
                    });
                }
                Ok(identifier_expr_with_pos(&name, token.line, token.column))
            }
            TokenType::Lparen => {
                self.advance();

                if let Some(closure) = self.try_parse_signature_closure()? {
                    return Ok(closure);
                }

                if self.check(TokenType::Ident) {
                    let mut type_name = self.peek().value.clone().unwrap_or_default();
                    if type_name.is_empty() {
                        type_name = "".to_string();
                    }
                    let has_type_args = self.peek_n(1).ttype == TokenType::Lbracket
                        && self.bracket_followed_by_dot();
                    let dot_offset = if has_type_args { 0 } else { 1 };
                    if !type_name.is_empty()
                        && ((has_type_args && self.peek_n(0).ttype == TokenType::Ident)
                            || self.peek_n(1).ttype == TokenType::Dot)
                        && self.peek_n(1 + dot_offset).ttype == TokenType::Dot
                    {
                        self.advance();
                        if has_type_args {
                            let targs = self.parse_type_args()?;
                            type_name = format!(
                                "{}[{}]",
                                type_name,
                                targs
                                    .iter()
                                    .map(data_type_name)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            );
                        }
                        self.advance();
                        let method_name = self.expect_member_name()?;
                        let full_name = format!("{}.{}", type_name, method_name);

                        let args = self.parse_call_arguments()?;
                        self.expect(TokenType::Rparen)?;
                        return Ok(Expression::Call {
                            name: full_name,
                            args,
                            type_args: Vec::new(),
                            name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
                        });
                    }
                }

                if self.check(TokenType::Ident) {
                    let first_token = self.peek();
                    let mut type_name = first_token.value.clone().unwrap_or_default();

                    if !type_name.contains('.') {
                        let has_targs = self.peek_n(1).ttype == TokenType::Lbracket
                            && self.bracket_followed_by_ident_colon();
                        if (has_targs || self.peek_n(1).ttype == TokenType::Ident)
                            && self.peek_n(if has_targs { 3 } else { 2 }).ttype == TokenType::Colon
                        {
                            self.advance();
                            if has_targs {
                                let targs = self.parse_type_args()?;
                                type_name = format!(
                                    "{}[{}]",
                                    type_name,
                                    targs
                                        .iter()
                                        .map(data_type_name)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                );
                            }

                            let mut args = Vec::new();

                            while !self.check(TokenType::Rparen) && !self.is_at_end() {
                                if self.check(TokenType::Ident)
                                    && self.peek_n(1).ttype == TokenType::Colon
                                {
                                    let field_name =
                                        self.advance().value.clone().unwrap_or_default();
                                    self.advance();
                                    let value_expr = self.parse_expression()?;
                                    args.push(Expression::NamedArg {
                                        name: field_name,
                                        value: Box::new(value_expr),
                                        data_type: DataType::Unknown,
                                    });

                                    if self.check(TokenType::Comma) {
                                        self.advance();
                                    }
                                } else {
                                    break;
                                }
                            }

                            self.expect(TokenType::Rparen)?;
                            return Ok(Expression::Call {
                                name: type_name,
                                args,
                                type_args: Vec::new(),
                                name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
                            });
                        }
                    }
                }

                let is_closure = (self.check(TokenType::Ident) || self.check(TokenType::SelfToken))
                    && self.peek_n(1).ttype == TokenType::Pipeline;

                if is_closure {
                    let param_name = if self.check(TokenType::SelfToken) {
                        self.advance();
                        "self".to_string()
                    } else {
                        self.expect_ident()?
                    };
                    self.advance();
                    let body = self.parse_closure_body()?;
                    self.expect(TokenType::Rparen)?;
                    return Ok(Expression::Closure {
                        params: vec![(param_name, DataType::Unknown)],
                        body,
                        return_type: DataType::Unknown,
                        capture: Vec::new(),
                    });
                }

                let expr = self.parse_expression()?;
                self.expect(TokenType::Rparen)?;
                Ok(expr)
            }
            TokenType::Lbracket => self.parse_bracket_literal(),
            TokenType::Lbrace => self.parse_brace_literal(),
            _ => Err(self.error("Unexpected token in expression")),
        }
    }

    fn check_keyword_ident(&self) -> bool {
        matches!(
            self.peek().ttype,
            TokenType::NewKw
                | TokenType::DropKw
                | TokenType::MoveKw
                | TokenType::OwnKw
                | TokenType::Set
                | TokenType::To
        )
    }

    fn parse_keyword_identifier(&mut self) -> Expression {
        let token = self.peek();
        let name = match token.ttype {
            TokenType::NewKw => "new",
            TokenType::DropKw => "drop",
            TokenType::MoveKw => "move",
            TokenType::OwnKw => "own",
            TokenType::Set => "set",
            TokenType::To => "to",
            _ => unreachable!(),
        };
        self.advance();
        identifier_expr_with_pos(name, token.line, token.column)
    }

    fn check_lifecycle_expression_prefix(&self) -> bool {
        matches!(
            self.peek().ttype,
            TokenType::NewKw | TokenType::OwnKw | TokenType::MoveKw | TokenType::DropKw
        ) && self.check_double_colon()
            && self.peek_n(2).ttype == TokenType::Lparen
    }

    fn parse_lifecycle_expression(&mut self) -> Result<Expression> {
        let name = match self.advance().ttype {
            TokenType::NewKw => "new::",
            TokenType::OwnKw => "own::",
            TokenType::MoveKw => "move::",
            TokenType::DropKw => "drop::",
            _ => return Err(self.error("Expected lifecycle keyword")),
        }
        .to_string();
        let args = self.parse_lifecycle_call_args()?;
        Ok(Expression::Call {
            name,
            args,
            type_args: Vec::new(),
            name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
        })
    }

    fn try_parse_signature_closure(&mut self) -> Result<Option<Expression>> {
        let start = self.pos;
        let params = match self.parse_param_list() {
            Ok(params) => params,
            Err(_) => {
                self.pos = start;
                return Ok(None);
            }
        };

        if !self.check(TokenType::Rparen) || self.peek_n(1).ttype != TokenType::Pipeline {
            self.pos = start;
            return Ok(None);
        }

        self.advance();
        self.advance();
        let body = self.parse_closure_body()?;

        Ok(Some(Expression::Closure {
            params,
            body,
            return_type: DataType::Unknown,
            capture: Vec::new(),
        }))
    }

    fn parse_closure_body(&mut self) -> Result<Vec<Statement>> {
        if self.check(TokenType::Lbrace) {
            self.advance();
            let mut stmts = self.parse_block()?;
            self.expect_block_close()?;
            if let Some(Statement::Expression(_)) = stmts.last()
                && let Statement::Expression(expr) = stmts.pop().unwrap()
            {
                stmts.push(Statement::Return(Some(expr)));
            }
            Ok(stmts)
        } else {
            let body_expr = self.parse_or()?;
            Ok(vec![Statement::Return(Some(body_expr))])
        }
    }

    fn parse_if_expression(&mut self) -> Result<Expression> {
        self.expect(TokenType::If)?;
        let condition = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        let then_expr = self.parse_expression_until_block_close()?;
        self.expect_block_close()?;
        self.expect(TokenType::Else)?;
        self.expect_block_open()?;
        let else_expr = self.parse_expression_until_block_close()?;
        self.expect_block_close()?;

        Ok(Expression::Call {
            name: "__if_expr".to_string(),
            args: vec![
                condition,
                Expression::Closure {
                    params: Vec::new(),
                    body: vec![Statement::Return(Some(then_expr))],
                    return_type: DataType::Unknown,
                    capture: Vec::new(),
                },
                Expression::Closure {
                    params: Vec::new(),
                    body: vec![Statement::Return(Some(else_expr))],
                    return_type: DataType::Unknown,
                    capture: Vec::new(),
                },
            ],
            type_args: Vec::new(),
            name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
        })
    }

}
