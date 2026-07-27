use crate::error::Result;
use crate::lexer::{Token, TokenType};
use crate::parser::ast::{AssignmentTarget, DataType, Expression, Identifier, Literal, Statement};

use super::Parser;
use super::helpers::{data_type_name, identifier_expr_with_pos, string_expr};
use super::syntax::contains_self_placeholder;
use super::syntax::replace_self_placeholder;

impl Parser {
    pub(super) fn parse_expression(&mut self) -> Result<Expression> {
        let expr = self.parse_pipeline_free_expression()?;
        self.parse_optional_type_ascription(expr)
    }

    pub(super) fn parse_pipeline_free_expression(&mut self) -> Result<Expression> {
        self.parse_or()
    }

    fn parse_optional_type_ascription(&mut self, expr: Expression) -> Result<Expression> {
        if !self.check(TokenType::Colon) {
            return Ok(expr);
        }

        self.advance();
        let data_type = self.parse_type()?;
        self.apply_type_ascription(expr, data_type)
    }

    fn apply_type_ascription(
        &self,
        mut expr: Expression,
        data_type: DataType,
    ) -> Result<Expression> {
        match &mut expr {
            Expression::Identifier(ident) => ident.data_type = data_type.clone(),
            Expression::BinaryOp {
                data_type: slot, ..
            }
            | Expression::UnaryOp {
                data_type: slot, ..
            }
            | Expression::NamedArg {
                data_type: slot, ..
            }
            | Expression::Call {
                name_line: 0,
            name_column: 0,
            data_type: slot, ..
            }
            | Expression::List {
                data_type: slot,
                ..
            }
            | Expression::Dict {
                data_type: slot, ..
            }
            | Expression::Index {
                data_type: slot, ..
            }
            | Expression::MemberAccess {
                data_type: slot, ..
            }
            | Expression::Pipeline {
                data_type: slot, ..
            }
            | Expression::Match {
                data_type: slot, ..
            }
            | Expression::EnumVariantPath {
                data_type: slot, ..
            }
            | Expression::EnumVariant {
                data_type: slot, ..
            }
            | Expression::Try {
                data_type: slot, ..
            }
            | Expression::Ok {
                data_type: slot, ..
            }
            | Expression::Err {
                data_type: slot, ..
            } => {
                *slot = data_type.clone();
            }
            Expression::Closure { return_type, .. } => {
                *return_type = data_type.clone();
            }
            // Literals (and any node without a mutable `data_type` slot) cannot
            // carry the ascription inline, so wrap them in an `Ascription` node
            // that the typechecker and lower turn into a real conversion.
            Expression::Literal { .. } => {
                return Ok(Expression::Ascription {
                    expr: Box::new(expr),
                    target: data_type.clone(),
                    data_type: data_type.clone(),
                });
            }
            _ => {}
        }

        if let Expression::List {
            elements,
            element_type,
            ..
        } = &mut expr
        {
            match &data_type {
                DataType::Array {
                    element_type: explicit,
                    ..
                }
                | DataType::Vector {
                    element_type: explicit,
                    ..
                }
                | DataType::Slice {
                    element_type: explicit,
                } => {
                    *element_type = *explicit.clone();
                }
                DataType::Map {
                    key_type,
                    value_type,
                } if elements.is_empty() => {
                    expr = Expression::Dict {
                        entries: Vec::new(),
                        key_type: (**key_type).clone(),
                        value_type: (**value_type).clone(),
                        data_type: data_type.clone(),
                    };
                    return Ok(expr);
                }
                _ => {}
            }
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expression> {
        let mut expr = self.parse_xor()?;
        self.skip_newlines();
        while self.check(TokenType::PipePipe) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_xor()?;
            expr = Expression::BinaryOp {
                operator: "||".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Bool,
            };
            self.skip_newlines();
        }
        Ok(expr)
    }

    fn parse_xor(&mut self) -> Result<Expression> {
        let mut expr = self.parse_and()?;
        self.skip_newlines();
        while self.check(TokenType::Xor) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            expr = Expression::BinaryOp {
                operator: "^".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Bool,
            };
            self.skip_newlines();
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expression> {
        let mut expr = self.parse_equality()?;
        self.skip_newlines();
        while self.check(TokenType::AmpAmp) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_equality()?;
            expr = Expression::BinaryOp {
                operator: "&&".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Bool,
            };
            self.skip_newlines();
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression> {
        let mut expr = self.parse_bitwise_or()?;
        loop {
            self.skip_newlines();
            if self.check(TokenType::Eq) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_bitwise_or()?;
                expr = Expression::BinaryOp {
                    operator: "==".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Neq) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_bitwise_or()?;
                expr = Expression::BinaryOp {
                    operator: "!=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Is) {
                self.advance();
                self.expect(TokenType::Lparen)?;
                let right = self.parse_bitwise_or()?;
                self.expect(TokenType::Rparen)?;
                expr = Expression::Call {
                    name: "__is".to_string(),
                    args: vec![expr, right],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::Bool,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expression> {
        let mut expr = self.parse_bitwise_and()?;
        self.skip_newlines();
        while self.check(TokenType::Pipe) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_bitwise_and()?;
            expr = Expression::BinaryOp {
                operator: "|".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Unknown,
            };
            self.skip_newlines();
        }
        Ok(expr)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expression> {
        let mut expr = self.parse_comparison()?;
        self.skip_newlines();
        while self.check(TokenType::Amp) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_comparison()?;
            expr = Expression::BinaryOp {
                operator: "&".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Unknown,
            };
            self.skip_newlines();
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression> {
        let mut expr = self.parse_additive()?;

        loop {
            self.skip_newlines();
            if self.check(TokenType::Pipeline) || self.check(TokenType::PipelineSafe) {
                let is_safe = self.check(TokenType::PipelineSafe);
                self.advance();
                self.skip_newlines();
                let stage = self.parse_additive()?;
                expr = self.apply_pipeline(expr, stage, is_safe)?;
                continue;
            }

            if self.check(TokenType::Gt) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: ">".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Lt) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: "<".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Gte) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: ">=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Lte) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: "<=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::In) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: "in".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Of) {
                self.advance();
                let ty = self.parse_type_name_string()?;
                expr = Expression::Call {
                    name: "__type_matches".to_string(),
                    args: vec![expr, string_expr(&ty)],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::Bool,
                };
            } else if self.check(TokenType::At) {
                self.advance();
                self.skip_newlines();
                let index = self.parse_additive()?;
                expr = Expression::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::To) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_additive()?;
                expr = Expression::Call {
                    name: "range".to_string(),
                    args: vec![expr, right],
                    type_args: Vec::new(),
                    name_line: 0,
            name_column: 0,
            data_type: DataType::List,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expression> {
        let mut expr = self.parse_shift()?;
        loop {
            self.skip_newlines();
            if self.check(TokenType::Plus) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_shift()?;
                expr = Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Minus) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_shift()?;
                expr = Expression::BinaryOp {
                    operator: "-".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_shift(&mut self) -> Result<Expression> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            self.skip_newlines();
            if self.check(TokenType::LShift) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_multiplicative()?;
                expr = Expression::BinaryOp {
                    operator: "<<".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::RShift) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_multiplicative()?;
                expr = Expression::BinaryOp {
                    operator: ">>".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression> {
        let mut expr = self.parse_unary()?;
        loop {
            self.skip_newlines();
            if self.check(TokenType::Star) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_unary()?;
                expr = Expression::BinaryOp {
                    operator: "*".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Slash) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_unary()?;
                expr = Expression::BinaryOp {
                    operator: "/".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Percent) {
                self.advance();
                self.skip_newlines();
                let right = self.parse_unary()?;
                expr = Expression::BinaryOp {
                    operator: "%".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expression> {
        if self.check(TokenType::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expression::UnaryOp {
                operator: "-".to_string(),
                operand: Box::new(operand),
                data_type: DataType::Unknown,
            });
        }

        if self.check(TokenType::Bang) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expression::UnaryOp {
                operator: "!".to_string(),
                operand: Box::new(operand),
                data_type: DataType::Bool,
            });
        }

        if self.check(TokenType::Amp) {
            self.advance();
            let is_mutable = self.check(TokenType::Mut);
            if is_mutable {
                self.expect(TokenType::Mut)?;
            }
            let expr = self.parse_unary()?;
            return Ok(Expression::Reference {
                expr: Box::new(expr),
                is_mutable,
                data_type: DataType::shared_ref(DataType::Unknown),
                referenced_type: DataType::Unknown,
            });
        }

        if self.check(TokenType::Star) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expression::Dereference {
                expr: Box::new(expr),
                data_type: DataType::Unknown,
            });
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expression> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(TokenType::Lbracket) && self.bracket_followed_by_lparen() {
                let call_target = match &expr {
                    Expression::Identifier(Identifier { name, .. }) => Some(name.clone()),
                    Expression::MemberAccess { .. } => Self::member_access_name(&expr),
                    _ => None,
                };
                if let Some(name) = call_target {
                    let type_args = self.parse_type_args()?;
                    if !self.check(TokenType::Lparen) {
                        return Err(self.error("'(' expected after type arguments"));
                    }
                    let args = self.parse_call_arguments()?;
                    expr = Expression::Call {
                        name,
                        args,
                        type_args,
                        name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
                    };
                    continue;
                }
            }

            if self.check(TokenType::Dot) {
                self.advance();
                let member = self.expect_member_name()?;
                expr = Expression::MemberAccess {
                    target: Box::new(expr),
                    member,
                    data_type: DataType::Unknown,
                };
                continue;
            }

            if self.check_double_colon() && Self::is_member_name_token(self.peek_n(2).ttype) {
                self.advance();
                self.advance();
                let member = self.expect_member_name()?;
                expr = Expression::MemberAccess {
                    target: Box::new(expr),
                    member,
                    data_type: DataType::Unknown,
                };
                continue;
            }

            if self.check(TokenType::Question) {
                self.advance();
                expr = Expression::Try {
                    expr: Box::new(expr),
                    data_type: DataType::Unknown,
                };
                continue;
            }

            if self.check(TokenType::Lparen) {
                let call_target = match &expr {
                    Expression::Identifier(Identifier { name, .. }) => Some(name.clone()),
                    Expression::MemberAccess { .. } => Self::member_access_name(&expr),
                    Expression::EnumVariantPath {
                        enum_name,
                        variant_name,
                        ..
                    } => Some(format!("{}.{}", enum_name, variant_name)),
                    _ => None,
                };
                if let Some(name) = call_target {
                    if name == "ok" || name == "err" || name == "some" {
                        let args = self.parse_call_arguments()?;
                        let value = if args.is_empty() {
                            Expression::Literal { lit: Literal::None, line: 0, column: 0 }
                        } else if args.len() == 1 {
                            args.into_iter().next().unwrap()
                        } else {
                            Expression::Tuple {
                                elements: args,
                                data_type: DataType::Unknown,
                            }
                        };
                        expr = if name == "ok" {
                            Expression::Ok {
                                value: Box::new(value),
                                data_type: DataType::Unknown,
                            }
                        } else if name == "err" {
                            Expression::Err {
                                value: Box::new(value),
                                data_type: DataType::Unknown,
                            }
                        } else {
                            Expression::Some {
                                value: Box::new(value),
                                data_type: DataType::Unknown,
                            }
                        };
                        continue;
                    }
                    let (name_line, name_column) = match &expr {
                        Expression::Identifier(ident) => (ident.line, ident.column),
                        _ => (0, 0),
                    };
                    if matches!(name.as_str(), "dasu" | "ireru") {
                        expr = self.parse_io_call(name)?;
                    } else {
                        let args = self.parse_call_arguments()?;
                        expr = Expression::Call {
                            name,
                            args,
                            type_args: Vec::new(),
                            name_line,
                            name_column,
                            data_type: DataType::Unknown,
                        };
                    }
                } else {
                    let args = self.parse_call_arguments()?;
                    let mut call_args = vec![expr];
                    call_args.extend(args);
                    expr = Expression::Call {
                        name: "call".to_string(),
                        args: call_args,
                        type_args: Vec::new(),
                        name_line: 0,
                        name_column: 0,
                        data_type: DataType::Unknown,
                    };
                }
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn bracket_followed_by_lparen(&self) -> bool {
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

    fn bracket_followed_by_dot(&self) -> bool {
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

    fn bracket_followed_by_ident_colon(&self) -> bool {
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

    fn parse_primary(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_use_expr(&mut self) -> Result<Expression> {
        self.expect(TokenType::Use)?;
        // `use! module::symbol(args)` — mandatory wrapper for calling a symbol
        // exposed by `load!`/`load`. The inner expression is the qualified
        // call; enforcement of the `use!` requirement happens during typeck.
        if self.check(TokenType::Bang) {
            self.advance();
            let expr = self.parse_pipeline_free_expression()?;
            return Ok(Expression::UseMacro {
                inner: Box::new(expr),
            });
        }
        let expr = self.parse_pipeline_free_expression()?;

        let result = if let Expression::Identifier(ident) = expr {
            Expression::Call {
                name: ident.name.clone(),
                args: Vec::new(),
                type_args: Vec::new(),
                name_line: 0,
            name_column: 0,
            data_type: DataType::Unknown,
            }
        } else {
            expr
        };
        let mut final_expr = result;
        while self.check(TokenType::Pipeline) || self.check(TokenType::PipelineSafe) {
            let is_safe = self.check(TokenType::PipelineSafe);
            self.advance();
            let stage = self.parse_pipeline_free_expression()?;
            final_expr = self.apply_pipeline(final_expr, stage, is_safe)?;
        }
        Ok(final_expr)
    }

    pub(super) fn parse_expression_until_block_open(&mut self) -> Result<Expression> {
        let slice = self.slice_until_block_boundary(super::BlockBoundary::Open);
        let mut parser = self.subparser_from_slice(slice);
        parser.parse_expression()
    }

    pub(super) fn parse_expression_until_block_close(&mut self) -> Result<Expression> {
        let slice = self.slice_until_block_boundary(super::BlockBoundary::Close);
        let mut parser = self.subparser_from_slice(slice);
        parser.parse_expression()
    }

    pub(super) fn parse_statements_until_block_close(&mut self) -> Result<Vec<Statement>> {
        let slice = self.slice_until_block_boundary(super::BlockBoundary::Close);
        let mut parser = self.subparser_from_slice(slice);
        parser.push_scope();
        Ok(parser.parse()?.statements)
    }

    fn slice_until_block_boundary(&mut self, boundary: super::BlockBoundary) -> Vec<Token> {
        let start = self.pos;
        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;
        let mut depth_brace = 0usize;

        while !self.is_at_end() {
            match self.peek().ttype {
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                TokenType::Lbrace if depth_paren == 0 && depth_bracket == 0 => match boundary {
                    super::BlockBoundary::Open if depth_brace == 0 => break,
                    super::BlockBoundary::Open | super::BlockBoundary::Close => depth_brace += 1,
                },
                TokenType::Rbrace if depth_paren == 0 && depth_bracket == 0 => {
                    if matches!(boundary, super::BlockBoundary::Close) && depth_brace == 0 {
                        break;
                    }
                    depth_brace = depth_brace.saturating_sub(1);
                }
                _ => {}
            }
            self.advance();
        }

        let end = self.pos;
        let mut slice = self.tokens[start..end].to_vec();
        slice.push(Token::new(
            TokenType::Eof,
            self.peek().line,
            self.peek().column,
        ));
        slice
    }

    fn subparser_from_slice(&self, slice: Vec<Token>) -> Parser {
        let mut parser = Parser::new(slice);
        parser.scopes = self.scopes.clone();
        parser.enum_names = self.enum_names.clone();
        parser.enum_variant_owners = self.enum_variant_owners.clone();
        parser.nominal_type_names = self.nominal_type_names.clone();
        parser.method_context = self.method_context;
        parser.type_param_scopes = self.type_param_scopes.clone();
        parser
    }

    fn parse_call_arguments(&mut self) -> Result<Vec<Expression>> {
        self.expect(TokenType::Lparen)?;
        let args = self.parse_expression_list_until(TokenType::Rparen)?;
        self.expect(TokenType::Rparen)?;
        Ok(args)
    }

    fn parse_enum_variant_arguments(&mut self) -> Result<Vec<Expression>> {
        self.expect(TokenType::Lparen)?;
        let mut args = Vec::new();
        while !self.check(TokenType::Rparen) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }

            if self.check(TokenType::Ident) && self.peek_n(1).ttype == TokenType::Colon {
                let name = self.expect_ident()?;
                self.expect(TokenType::Colon)?;
                args.push(Expression::NamedArg {
                    name,
                    value: Box::new(self.parse_expression()?),
                    data_type: DataType::Unknown,
                });
            } else {
                args.push(self.parse_expression()?);
            }

            if self.check(TokenType::Comma) {
                self.advance();
            }
        }
        self.expect(TokenType::Rparen)?;
        Ok(args)
    }

    pub(super) fn parse_expression_list_until(
        &mut self,
        terminator: TokenType,
    ) -> Result<Vec<Expression>> {
        let mut args = Vec::new();
        while !self.check(terminator) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }

            if self.check(TokenType::Ident)
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|tok| tok.ttype == TokenType::Assign)
            {
                let name = self.expect_ident()?;
                self.expect(TokenType::Assign)?;
                args.push(Expression::NamedArg {
                    name,
                    value: Box::new(self.parse_expression()?),
                    data_type: DataType::Unknown,
                });
            } else {
                args.push(self.parse_expression()?);
            }

            if self.check(TokenType::Comma) {
                self.advance();
            }
        }
        Ok(args)
    }

    fn parse_io_call(&mut self, name: String) -> Result<Expression> {
        self.expect(TokenType::Lparen)?;
        let mut args = self.parse_expression_list_until(TokenType::Rparen)?;
        self.expect(TokenType::Rparen)?;

        for arg in &mut args {
            self.normalize_io_argument(arg)?;
        }

        let data_type = if self.check(TokenType::Colon) {
            self.advance();
            self.parse_type()?
        } else {
            DataType::Str
        };

        Ok(Expression::Call {
            name,
            args,
            type_args: Vec::new(),
            name_line: 0,
            name_column: 0,
            data_type,
        })
    }

    fn normalize_io_argument(&self, expr: &mut Expression) -> Result<()> {
        if let Expression::Literal { lit: Literal::Str(value), .. } = expr
            && value.contains('{')
        {
            *expr = super::helpers::concat_expressions(self.parse_string_template_parts(value)?);
        }

        Ok(())
    }

    fn parse_string_template_parts(&self, value: &str) -> Result<Vec<Expression>> {
        let mut parts = Vec::new();

        if !value.contains('{') {
            parts.push(string_expr(value));
            return Ok(parts);
        }

        let bytes = value.as_bytes();
        let mut i = 0;
        let mut current_text = String::new();

        while i < bytes.len() {
            let b = bytes[i];

            if b == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                current_text.push('{');
                i += 2;
            } else if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                current_text.push('}');
                i += 2;
            } else if b == b'{' {
                if !current_text.is_empty() {
                    parts.push(string_expr(&current_text));
                    current_text.clear();
                }

                let inner_start = i + 1;
                let mut depth = 1;
                let mut paren_depth = 0;
                i += 1;

                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' if paren_depth == 0 => {
                            depth -= 1;
                        }
                        b'(' => paren_depth += 1,
                        b')' if paren_depth > 0 => paren_depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }

                if depth != 0 {
                    return Err(self.error("Unclosed interpolation in template string"));
                }

                let inner = &value[inner_start..i - 1];
                let interp = self.parse_interpolation_source(inner)?;
                parts.push(interp);
            } else {
                current_text.push(b as char);
                i += 1;
            }
        }

        if !current_text.is_empty() {
            parts.push(string_expr(&current_text));
        }

        if parts.is_empty() {
            parts.push(string_expr(value));
        }

        Ok(parts)
    }

    fn parse_interpolation_source(&self, source: &str) -> Result<Expression> {
        let mut parser = Parser::new(crate::lexer::tokenize(source)?);
        parser.scopes = self.scopes.clone();
        parser.enum_names = self.enum_names.clone();
        parser.enum_variant_owners = self.enum_variant_owners.clone();
        parser.nominal_type_names = self.nominal_type_names.clone();
        parser.method_context = self.method_context;

        let expr = parser.parse_expression()?;
        if parser.check(TokenType::Colon) {
            parser.advance();
            let mut spec = String::new();
            while !parser.is_at_end() {
                let token = parser.advance();
                spec.push_str(&parser.token_surface(token));
            }
            return Ok(Expression::Call {
                name: "__mire_fmt".to_string(),
                args: vec![expr, string_expr(&spec)],
                type_args: Vec::new(),
                name_line: 0,
            name_column: 0,
            data_type: DataType::Str,
            });
        }

        parser.skip_newlines();
        if !parser.is_at_end() {
            return Err(self.error("Invalid interpolation in template string"));
        }

        Ok(Expression::Call {
            name: "str".to_string(),
            args: vec![expr],
            type_args: Vec::new(),
            name_line: 0,
            name_column: 0,
            data_type: DataType::Str,
        })
    }

    pub(super) fn parse_param_list(&mut self) -> Result<Vec<(String, DataType)>> {
        let mut params = Vec::new();
        while !self.check(TokenType::Rparen) && !self.is_at_end() {
            let name = if self.check(TokenType::SelfToken) {
                self.advance();
                "self".to_string()
            } else {
                self.expect_ident()?
            };

            let data_type = if self.check(TokenType::Colon) {
                self.advance();
                let data_type = self.parse_type()?;
                if self.check(TokenType::Mut) {
                    self.advance();
                }
                data_type
            } else {
                DataType::Unknown
            };

            params.push((name, data_type));
            if self.check(TokenType::Comma) {
                self.advance();
            }
        }
        Ok(params)
    }

    fn parse_bracket_literal(&mut self) -> Result<Expression> {
        self.expect(TokenType::Lbracket)?;
        if self.check(TokenType::Rbracket) {
            self.advance();
            return Ok(Expression::List {
                elements: Vec::new(),
                element_type: DataType::Unknown,
                data_type: DataType::List,
            });
        }

        let contains_comma = self.bracket_contains_top_level_comma();
        if contains_comma {
            let mut entries = Vec::new();
            while !self.check(TokenType::Rbracket) && !self.is_at_end() {
                let parsed_key = self.parse_pipeline_free_expression()?;
                let key = self.coerce_dict_key_to_string(parsed_key);
                let value = self.parse_pipeline_free_expression()?;
                entries.push((key, value));
                if self.check(TokenType::Comma) {
                    self.advance();
                }
            }
            self.expect(TokenType::Rbracket)?;
            Ok(Expression::Dict {
                entries,
                key_type: DataType::Unknown,
                value_type: DataType::Unknown,
                data_type: DataType::Dict,
            })
        } else {
            let mut elements = Vec::new();
            while !self.check(TokenType::Rbracket) && !self.is_at_end() {
                elements.push(self.parse_expression()?);
            }
            self.expect(TokenType::Rbracket)?;
            Ok(Expression::List {
                elements,
                element_type: DataType::Unknown,
                data_type: DataType::List,
            })
        }
    }

    fn parse_brace_literal(&mut self) -> Result<Expression> {
        self.expect(TokenType::Lbrace)?;
        let mut entries = Vec::new();

        while !self.check(TokenType::Rbrace) && !self.is_at_end() {
            let parsed_key = self.parse_pipeline_free_expression()?;
            let key = self.coerce_dict_key_to_string(parsed_key);
            self.expect(TokenType::Colon)?;
            let value = self.parse_pipeline_free_expression()?;
            entries.push((key, value));

            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
        }

        self.expect(TokenType::Rbrace)?;
        Ok(Expression::Dict {
            entries,
            key_type: DataType::Unknown,
            value_type: DataType::Unknown,
            data_type: DataType::Dict,
        })
    }

    fn apply_pipeline(
        &self,
        input: Expression,
        stage: Expression,
        safe: bool,
    ) -> Result<Expression> {
        let had_self_placeholder = contains_self_placeholder(&stage);
        let processed_stage = if had_self_placeholder {
            replace_self_placeholder(stage, &input)
        } else {
            stage
        };

        if had_self_placeholder && !safe {
            return Ok(processed_stage);
        }

        Ok(Expression::Pipeline {
            input: Box::new(input),
            stage: Box::new(processed_stage),
            safe,
            data_type: DataType::Unknown,
        })
    }

    pub(super) fn parse_assignment_target(&mut self) -> Result<AssignmentTarget> {
        let mut target = if self.check(TokenType::SelfToken) {
            self.advance();
            "self".to_string()
        } else {
            self.expect_ident()?
        };

        while self.check(TokenType::Dot) {
            self.advance();
            target.push('.');
            target.push_str(&self.expect_ident()?);
        }

        let base = if target.contains('.') {
            AssignmentTarget::Field(target)
        } else {
            AssignmentTarget::Variable(target)
        };

        if self.check(TokenType::At) || self.check(TokenType::Lbracket) {
            let index = if self.check(TokenType::At) {
                self.advance();
                self.parse_additive()?
            } else {
                self.advance();
                let index = self.parse_expression()?;
                self.expect(TokenType::Rbracket)?;
                index
            };
            Ok(AssignmentTarget::Index {
                target: Box::new(base.as_expression()),
                index: Box::new(index),
            })
        } else {
            Ok(base)
        }
    }
}
