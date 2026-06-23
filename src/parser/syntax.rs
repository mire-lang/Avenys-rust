use crate::error::Result;
use crate::lexer::TokenType;
use crate::parser::Parser;
use crate::parser::ast::{DataType, Expression, Identifier, Literal, Statement};
use crate::parser::helpers::identifier_expr_with_pos;

pub(super) fn contains_self_placeholder(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(Identifier { name, .. }) => name == "self",
        Expression::BinaryOp { left, right, .. } => {
            contains_self_placeholder(left) || contains_self_placeholder(right)
        }
        Expression::UnaryOp { operand, .. } => contains_self_placeholder(operand),
        Expression::NamedArg { value, .. } => contains_self_placeholder(value),
        Expression::Call { args, .. }
        | Expression::List { elements: args, .. }
        | Expression::Tuple { elements: args, .. } => args.iter().any(contains_self_placeholder),
        Expression::Dict { entries, .. } => entries
            .iter()
            .any(|(key, value)| contains_self_placeholder(key) || contains_self_placeholder(value)),
        Expression::Index { target, index, .. } => {
            contains_self_placeholder(target) || contains_self_placeholder(index)
        }
        Expression::MemberAccess { target, .. }
        | Expression::Dereference { expr: target, .. }
        | Expression::Reference { expr: target, .. }
        | Expression::Box { value: target, .. } => contains_self_placeholder(target),
        Expression::Closure { body, .. } => body.iter().any(statement_contains_self_placeholder),
        Expression::Literal(_) => false,
        Expression::Pipeline { input, stage, .. } => {
            contains_self_placeholder(input) || contains_self_placeholder(stage)
        }
        Expression::Match {
            value,
            cases,
            default,
            ..
        } => {
            contains_self_placeholder(value)
                || cases
                    .iter()
                    .any(|(p, r)| contains_self_placeholder(p) || contains_self_placeholder(r))
                || contains_self_placeholder(default)
        }
        Expression::EnumVariantPath { .. } => false,
        Expression::EnumVariant { payloads, .. } => payloads.iter().any(contains_self_placeholder),
        Expression::Try { expr, .. } => contains_self_placeholder(expr),
        Expression::Ok { value, .. } | Expression::Err { value, .. } => {
            contains_self_placeholder(value)
        }
    }
}

fn statement_contains_self_placeholder(statement: &Statement) -> bool {
    match statement {
        Statement::Let { value, .. } => value.as_ref().is_some_and(contains_self_placeholder),
        Statement::Assignment { target, value, .. } => {
            contains_self_placeholder(&target.as_expression()) || contains_self_placeholder(value)
        }
        Statement::Function { body, .. }
        | Statement::Unsafe { body } => body.iter().any(statement_contains_self_placeholder),
        Statement::Return(expr) => expr.as_ref().is_some_and(contains_self_placeholder),
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            contains_self_placeholder(condition)
                || then_branch.iter().any(statement_contains_self_placeholder)
                || else_branch
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_contains_self_placeholder))
        }
        Statement::While { condition, body } => {
            contains_self_placeholder(condition)
                || body.iter().any(statement_contains_self_placeholder)
        }
        Statement::For { iterable, body, .. } | Statement::Find { iterable, body, .. } => {
            contains_self_placeholder(iterable)
                || body.iter().any(statement_contains_self_placeholder)
        }
        Statement::Expression(expr) => contains_self_placeholder(expr),
        Statement::Match {
            value,
            cases,
            default,
        } => {
            contains_self_placeholder(value)
                || cases.iter().any(|(expr, body)| {
                    contains_self_placeholder(expr)
                        || body.iter().any(statement_contains_self_placeholder)
                })
                || default.iter().any(statement_contains_self_placeholder)
        }
        Statement::Impl { methods, .. } => methods.iter().any(statement_contains_self_placeholder),
        Statement::Type { fields, .. } => fields.iter().any(statement_contains_self_placeholder),
        Statement::Skill { .. } => false,
        Statement::Asm { instructions } => instructions
            .iter()
            .any(|(_, expr)| contains_self_placeholder(expr)),
        Statement::Drop { value } => contains_self_placeholder(value),
        Statement::New { value, .. } | Statement::Own { value, .. } => {
            value.as_ref().is_some_and(contains_self_placeholder)
        }
        Statement::Move { value, .. } => contains_self_placeholder(value),
        Statement::Query { .. }
        | Statement::Break
        | Statement::Continue
        | Statement::ExternLib { .. }
        | Statement::ExternFunction { .. }
        | Statement::Load { .. }
        | Statement::Enum { .. }
        | Statement::Module { .. }
        => false,
    }
}

pub(super) fn replace_self_placeholder(expr: Expression, replacement: &Expression) -> Expression {
    match expr {
        Expression::Identifier(Identifier { name, .. }) if name == "self" => replacement.clone(),
        Expression::BinaryOp {
            operator,
            left,
            right,
            data_type,
        } => Expression::BinaryOp {
            operator,
            left: Box::new(replace_self_placeholder(*left, replacement)),
            right: Box::new(replace_self_placeholder(*right, replacement)),
            data_type,
        },
        Expression::UnaryOp {
            operator,
            operand,
            data_type,
        } => Expression::UnaryOp {
            operator,
            operand: Box::new(replace_self_placeholder(*operand, replacement)),
            data_type,
        },
        Expression::NamedArg {
            name,
            value,
            data_type,
        } => Expression::NamedArg {
            name,
            value: Box::new(replace_self_placeholder(*value, replacement)),
            data_type,
        },
        Expression::Call {
            name,
            args,
            type_args,
            data_type,
        } => Expression::Call {
            name,
            args: args
                .into_iter()
                .map(|arg| replace_self_placeholder(arg, replacement))
                .collect(),
            type_args,
            data_type,
        },
        Expression::List {
            elements,
            element_type,
            data_type,
        } => Expression::List {
            elements: elements
                .into_iter()
                .map(|arg| replace_self_placeholder(arg, replacement))
                .collect(),
            element_type,
            data_type,
        },
        Expression::Tuple {
            elements,
            data_type,
        } => Expression::Tuple {
            elements: elements
                .into_iter()
                .map(|arg| replace_self_placeholder(arg, replacement))
                .collect(),
            data_type,
        },
        Expression::Dict {
            entries,
            key_type,
            value_type,
            data_type,
        } => Expression::Dict {
            entries: entries
                .into_iter()
                .map(|(key, value)| {
                    (
                        replace_self_placeholder(key, replacement),
                        replace_self_placeholder(value, replacement),
                    )
                })
                .collect(),
            key_type,
            value_type,
            data_type,
        },
        Expression::Index {
            target,
            index,
            data_type,
        } => Expression::Index {
            target: Box::new(replace_self_placeholder(*target, replacement)),
            index: Box::new(replace_self_placeholder(*index, replacement)),
            data_type,
        },
        Expression::MemberAccess {
            target,
            member,
            data_type,
        } => Expression::MemberAccess {
            target: Box::new(replace_self_placeholder(*target, replacement)),
            member,
            data_type,
        },
        Expression::Reference {
            expr,
            is_mutable,
            data_type,
            referenced_type,
        } => Expression::Reference {
            expr: Box::new(replace_self_placeholder(*expr, replacement)),
            is_mutable,
            data_type,
            referenced_type,
        },
        Expression::Dereference { expr, data_type } => Expression::Dereference {
            expr: Box::new(replace_self_placeholder(*expr, replacement)),
            data_type,
        },
        Expression::Box { value, data_type } => Expression::Box {
            value: Box::new(replace_self_placeholder(*value, replacement)),
            data_type,
        },
        Expression::Pipeline {
            input,
            stage,
            safe,
            data_type,
        } => Expression::Pipeline {
            input: Box::new(replace_self_placeholder(*input, replacement)),
            stage: Box::new(replace_self_placeholder(*stage, replacement)),
            safe,
            data_type,
        },
        Expression::Try { expr, data_type } => Expression::Try {
            expr: Box::new(replace_self_placeholder(*expr, replacement)),
            data_type,
        },
        Expression::Ok { value, data_type } => Expression::Ok {
            value: Box::new(replace_self_placeholder(*value, replacement)),
            data_type,
        },
        Expression::Err { value, data_type } => Expression::Err {
            value: Box::new(replace_self_placeholder(*value, replacement)),
            data_type,
        },
        Expression::Match {
            value,
            cases,
            default,
            data_type,
        } => Expression::Match {
            value: Box::new(replace_self_placeholder(*value, replacement)),
            cases: cases
                .into_iter()
                .map(|(p, r)| {
                    (
                        replace_self_placeholder(p, replacement),
                        replace_self_placeholder(r, replacement),
                    )
                })
                .collect(),
            default: Box::new(replace_self_placeholder(*default, replacement)),
            data_type,
        },
        Expression::EnumVariant {
            enum_name,
            variant_name,
            payloads,
            data_type,
        } => Expression::EnumVariant {
            enum_name,
            variant_name,
            payloads: payloads
                .into_iter()
                .map(|payload| replace_self_placeholder(payload, replacement))
                .collect(),
            data_type,
        },
        other => other,
    }
}

impl Parser {
    pub(super) fn parse_lifecycle_call_args(&mut self) -> Result<Vec<Expression>> {
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::Lparen)?;
        let mut args = Vec::new();
        while !self.check(TokenType::Rparen) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
            args.push(self.parse_expression()?);
            if self.check(TokenType::Comma) {
                self.advance();
            }
        }
        self.expect(TokenType::Rparen)?;
        Ok(args)
    }

    pub(super) fn parse_new_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::NewKw)?;
        let mut args = self.parse_lifecycle_call_args()?;
        self.expect(TokenType::Colon)?;
        let declared_type = self.parse_type()?;
        let value = if args.is_empty() {
            None
        } else if args.len() == 1 {
            Some(args.remove(0))
        } else {
            Some(Expression::Tuple {
                elements: args,
                data_type: DataType::Unknown,
            })
        };
        Ok(Statement::New {
            value,
            declared_type,
        })
    }

    pub(super) fn parse_own_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::OwnKw)?;
        let mut args = self.parse_lifecycle_call_args()?;
        self.expect(TokenType::Colon)?;
        let inner_type = self.parse_type()?;
        let value = if args.is_empty() {
            None
        } else if args.len() == 1 {
            Some(args.remove(0))
        } else {
            Some(Expression::Tuple {
                elements: args,
                data_type: DataType::Unknown,
            })
        };
        Ok(Statement::Own { value, inner_type })
    }

    pub(super) fn parse_move_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::MoveKw)?;
        let mut args = self.parse_lifecycle_call_args()?;
        if args.len() != 1 {
            return Err(self.error("move:: expects exactly one source expression"));
        }
        self.expect(TokenType::To)?;
        let target = self.expect_ident()?;
        Ok(Statement::Move {
            target,
            value: args.remove(0),
        })
    }

    pub(super) fn parse_drop_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::DropKw)?;
        let mut args = self.parse_lifecycle_call_args()?;
        let value = if args.is_empty() {
            return Err(self.error("drop:: expects at least one expression"));
        } else if args.len() == 1 {
            args.remove(0)
        } else {
            Expression::Tuple {
                elements: args,
                data_type: DataType::Unknown,
            }
        };
        Ok(Statement::Drop { value })
    }
}

impl Parser {
    pub(super) fn parse_match_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Match)?;
        let value = self.parse_match_value()?;
        self.expect(TokenType::Lbrace)?;

        let mut cases = Vec::new();
        let mut default = Vec::new();
        let mut seen_default = false;
        let mut consumed_closing_brace = false;

        loop {
            self.skip_newlines();
            if self.check(TokenType::Rbrace) {
                self.advance();
                consumed_closing_brace = true;
                break;
            }
            if !self.next_tokens_form_match_case() {
                break;
            }

            let mut pattern = self.parse_match_pattern()?;
            if self.check(TokenType::Ident) && self.peek().value.as_deref() == Some("when") {
                self.advance();
                let guard = self.parse_expression_until_block_open()?;
                pattern = Expression::Call {
                    name: "__match_guard".to_string(),
                    args: vec![pattern, guard],
                    type_args: Vec::new(),
                    data_type: DataType::Bool,
                };
            }
            self.skip_newlines();
            self.expect(TokenType::Lbrace)?;
            self.skip_newlines();
            self.push_scope();
            self.declare_match_pattern_bindings(&pattern);
            let body = self.parse_statements_until_block_close()?;
            self.pop_scope();
            self.expect(TokenType::Rbrace)?;

            let is_default = matches!(
                &pattern,
                Expression::Identifier(Identifier { name, .. }) if name == "_"
            );
            if is_default {
                if seen_default {
                    return Err(
                        self.error("match statement cannot contain multiple default '_' arms")
                    );
                }
                seen_default = true;
                default = body;
            } else {
                if seen_default {
                    return Err(
                        self.error("match statement cases cannot appear after default '_' arm")
                    );
                }
                cases.push((pattern, body));
            }
        }

        if !consumed_closing_brace {
            self.expect(TokenType::Rbrace)?;
        }

        Ok(Statement::Match {
            value,
            cases,
            default,
        })
    }

    pub(super) fn parse_match_expression(&mut self) -> Result<Expression> {
        let value = self.parse_match_value()?;
        self.skip_newlines();
        self.expect(TokenType::Lbrace)?;
        self.skip_newlines();

        let mut cases = Vec::new();
        let mut default = None;
        let mut seen_default = false;
        let mut consumed_closing_brace = false;

        loop {
            self.skip_newlines();
            if self.check(TokenType::Rbrace) {
                self.advance();
                consumed_closing_brace = true;
                break;
            }
            if !self.next_tokens_form_match_case() {
                break;
            }

            let mut pattern_expr = self.parse_match_pattern()?;
            if self.check(TokenType::Ident) && self.peek().value.as_deref() == Some("when") {
                self.advance();
                let guard = self.parse_expression_until_block_open()?;
                pattern_expr = Expression::Call {
                    name: "__match_guard".to_string(),
                    args: vec![pattern_expr, guard],
                    type_args: Vec::new(),
                    data_type: DataType::Bool,
                };
            }
            self.skip_newlines();
            self.expect(TokenType::Lbrace)?;
            self.skip_newlines();
            self.push_scope();
            self.declare_match_pattern_bindings(&pattern_expr);
            let body_expr = self.parse_expression_until_block_close()?;
            self.pop_scope();
            self.expect(TokenType::Rbrace)?;

            let is_default = matches!(
                &pattern_expr,
                Expression::Identifier(Identifier { name, .. }) if name == "_"
            );
            if is_default {
                if seen_default {
                    return Err(
                        self.error("match expression cannot contain multiple default '_' arms")
                    );
                }
                seen_default = true;
                default = Some(body_expr);
            } else {
                if seen_default {
                    return Err(
                        self.error("match expression cases cannot appear after default '_' arm")
                    );
                }
                cases.push((pattern_expr, body_expr));
            }

            self.skip_newlines();
        }

        if !consumed_closing_brace {
            self.expect(TokenType::Rbrace)?;
        }

        Ok(Expression::Match {
            value: Box::new(value),
            cases,
            default: Box::new(default.unwrap_or(Expression::Literal(Literal::None))),
            data_type: DataType::Unknown,
        })
    }

    fn parse_match_pattern(&mut self) -> Result<Expression> {
        let mut pattern = self.parse_match_pattern_atom()?;
        while self.check(TokenType::Pipe) {
            self.advance();
            let rhs = self.parse_match_pattern_atom()?;
            pattern = Expression::Call {
                name: "__match_or".to_string(),
                args: vec![pattern, rhs],
                type_args: Vec::new(),
                data_type: DataType::Bool,
            };
        }
        Ok(pattern)
    }

    fn parse_match_pattern_atom(&mut self) -> Result<Expression> {
        let token = self.peek();
        match token.ttype {
            TokenType::Ident => {
                let name = self.advance().value.unwrap_or_default();
                if self.check(TokenType::Dot) && self.peek_n(1).ttype == TokenType::Ident {
                    self.advance();
                    let variant_name = self.advance().value.unwrap_or_default();
                    if self.check(TokenType::Lparen) {
                        self.advance();
                        let payloads = self.parse_expression_list_until(TokenType::Rparen)?;
                        self.expect(TokenType::Rparen)?;
                        let enum_name = name;
                        return Ok(Expression::EnumVariant {
                            enum_name: enum_name.clone(),
                            variant_name,
                            payloads,
                            data_type: DataType::EnumNamed(enum_name),
                        });
                    }
                    let enum_name = name;
                    return Ok(Expression::EnumVariantPath {
                        enum_name: enum_name.clone(),
                        variant_name,
                        data_type: DataType::EnumNamed(enum_name),
                    });
                }

                if self.check(TokenType::Lparen)
                    && name.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    self.advance();
                    let payloads = self.parse_expression_list_until(TokenType::Rparen)?;
                    self.expect(TokenType::Rparen)?;
                    let Some(enum_name) = self.enum_variant_owners.get(&name).cloned() else {
                        return Err(self.error_at(
                            token.line,
                            token.column,
                            &format!(
                                "Cannot resolve enum variant shorthand '{}'; use Enum.{} explicitly",
                                name, name
                            ),
                        ));
                    };
                    return Ok(Expression::EnumVariant {
                        enum_name: enum_name.clone(),
                        variant_name: name,
                        payloads,
                        data_type: DataType::EnumNamed(enum_name),
                    });
                }

                Ok(identifier_expr_with_pos(&name, token.line, token.column))
            }
            TokenType::IntLit => {
                let token = self.advance();
                let val = token.value.unwrap_or_default();
                let parsed = val.parse().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid integer literal '{}'", val),
                    )
                })?;
                let base = Expression::Literal(Literal::Int(parsed));
                if self.check(TokenType::Dot) && self.peek_n(1).ttype == TokenType::Dot {
                    self.advance();
                    self.advance();
                    let end = self.parse_match_pattern_atom()?;
                    return Ok(Expression::Call {
                        name: "__match_range".to_string(),
                        args: vec![base, end],
                        type_args: Vec::new(),
                        data_type: DataType::Bool,
                    });
                }
                Ok(base)
            }
            TokenType::FloatLit => {
                let token = self.advance();
                let val = token.value.unwrap_or_default();
                let parsed = val.parse().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid float literal '{}'", val),
                    )
                })?;
                Ok(Expression::Literal(Literal::Float(parsed)))
            }
            TokenType::CharLit => {
                let token = self.advance();
                let val = token.value.unwrap_or_default();
                let parsed = val.parse::<u32>().map_err(|_| {
                    self.error_at(
                        token.line,
                        token.column,
                        &format!("Invalid char literal '{}'", val),
                    )
                })?;
                Ok(Expression::Literal(Literal::Char(parsed)))
            }
            TokenType::StrLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Str(val)))
            }
            TokenType::BoolLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Bool(val == "true")))
            }
            _ => Err(self.error("Expected pattern in match case")),
        }
    }

    fn parse_match_value(&mut self) -> Result<Expression> {
        while self.peek().ttype == TokenType::Newline {
            self.advance();
        }
        self.parse_expression()
    }

    fn next_tokens_form_match_case(&self) -> bool {
        let mut i = self.pos;
        while let Some(token) = self.tokens.get(i) {
            if token.ttype != TokenType::Newline {
                break;
            }
            i += 1;
        }

        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;
        let mut index = i;
        while let Some(token) = self.tokens.get(index) {
            match token.ttype {
                TokenType::Rbrace if depth_paren == 0 && depth_bracket == 0 => return false,
                TokenType::Dot if depth_paren == 0 && depth_bracket == 0 => {
                    index += 1;
                    continue;
                }
                TokenType::Colon | TokenType::Eof if depth_paren == 0 && depth_bracket == 0 => {
                    return false;
                }
                TokenType::Newline => {
                    index += 1;
                    continue;
                }
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                TokenType::IntLit
                | TokenType::FloatLit
                | TokenType::CharLit
                | TokenType::StrLit
                | TokenType::BoolLit
                | TokenType::NoneLit
                    if depth_paren == 0 && depth_bracket == 0 =>
                {
                    return true;
                }
                TokenType::Ident if depth_paren == 0 && depth_bracket == 0 => return true,
                _ => {}
            }
            index += 1;
        }
        false
    }

    fn declare_match_pattern_bindings(&mut self, pattern: &Expression) {
        match pattern {
            Expression::EnumVariant { payloads, .. } => {
                for payload in payloads {
                    if let Expression::Identifier(Identifier { name, .. }) = payload {
                        self.declare(name);
                    }
                }
            }
            Expression::Call { name, args, .. } if name == "__match_guard" => {
                if let Some(inner) = args.first() {
                    self.declare_match_pattern_bindings(inner);
                }
            }
            Expression::Call { name, args, .. } if name == "__match_or" => {
                if let Some(first) = args.first() {
                    self.declare_match_pattern_bindings(first);
                }
            }
            _ => {}
        }
    }
}
