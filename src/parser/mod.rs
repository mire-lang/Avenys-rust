pub mod ast;

use crate::error::{ErrorKind, MireError, Result};
use crate::lexer::{tokenize, Token, TokenType};
use crate::parser::ast::{
    DataType, Expression, Identifier, Literal, Statement, TraitMethodSig, Visibility,
};
use std::collections::HashSet;

pub use ast::{EnumDef, EnumVariantDef, MireValue, Program};

pub fn parse(source: &str) -> Result<Program> {
    let tokens = tokenize(source)?;
    Parser::new(tokens).parse()
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    scopes: Vec<HashSet<String>>,
    method_context: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            scopes: vec![HashSet::new()],
            method_context: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Program> {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }
        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        match self.peek().ttype {
            TokenType::Import => self.parse_import_statement(),
            TokenType::Set => self.parse_set_statement(),
            TokenType::Use => Ok(Statement::Expression(self.parse_use_expression()?)),
            TokenType::Pub | TokenType::Priv => {
                let visibility = self.parse_visibility()?;
                match self.peek().ttype {
                    TokenType::Fn => self.parse_fn_statement(visibility),
                    TokenType::Type => self.parse_type_statement(),
                    TokenType::Skill => self.parse_skill_statement(),
                    TokenType::Struct => self.parse_struct_statement(),
                    TokenType::Trait => self.parse_trait_statement(),
                    TokenType::Enum => self.parse_enum_statement(),
                    _ => {
                        Err(self
                            .error("Expected fn, type, skill, struct, or enum after visibility"))
                    }
                }
            }
            TokenType::Fn => self.parse_fn_statement(Visibility::Private),
            TokenType::Type => self.parse_type_statement(),
            TokenType::Skill => self.parse_skill_statement(),
            TokenType::Code => self.parse_code_statement(),
            TokenType::Struct => self.parse_struct_statement(),
            TokenType::Impl => self.parse_impl_statement(),
            TokenType::Trait => self.parse_trait_statement(),
            TokenType::Enum => self.parse_enum_statement(),
            TokenType::If => self.parse_if_statement(),
            TokenType::While => self.parse_while_statement(),
            TokenType::For => self.parse_for_statement(),
            TokenType::Do => self.parse_do_while_statement(),
            TokenType::Match => self.parse_match_statement(),
            TokenType::Return => self.parse_return_statement(),
            TokenType::Break => {
                self.advance();
                Ok(Statement::Break)
            }
            TokenType::Continue => {
                self.advance();
                Ok(Statement::Continue)
            }
            _ => Ok(Statement::Expression(self.parse_expression()?)),
        }
    }

    fn parse_import_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Import)?;
        let (path, is_local) = self.parse_import_path()?;

        let alias = if self.check(TokenType::As) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let items = if self.check(TokenType::Colon) {
            self.advance();
            self.expect(TokenType::Lparen)?;
            let mut items = Vec::new();
            while !self.check(TokenType::Rparen) && !self.is_at_end() {
                items.push(self.expect_ident()?);
            }
            self.expect(TokenType::Rparen)?;
            Some(items)
        } else {
            None
        };

        if is_local && alias.is_some() {
            return Err(self.error("Local import statements do not support aliasing"));
        }

        Ok(Statement::Use {
            path,
            alias,
            items,
            is_local,
        })
    }

    fn parse_import_path(&mut self) -> Result<(String, bool)> {
        if self.check(TokenType::Dot) {
            self.advance();
            self.expect(TokenType::Slash)?;
            let mut path = String::from("./");
            path.push_str(&self.expect_member_name()?);
            while self.check(TokenType::Slash) {
                self.advance();
                path.push('/');
                path.push_str(&self.expect_member_name()?);
            }
            return Ok((path, true));
        }

        Ok((self.expect_ident()?, false))
    }

    fn parse_set_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Set)?;

        let target = self.parse_assignment_target()?;
        let op = self.advance();
        let is_compound = matches!(
            op.ttype,
            TokenType::PlusAssign
                | TokenType::MinusAssign
                | TokenType::StarAssign
                | TokenType::SlashAssign
                | TokenType::PercentAssign
        );

        if !matches!(
            op.ttype,
            TokenType::Assign
                | TokenType::PlusAssign
                | TokenType::MinusAssign
                | TokenType::StarAssign
                | TokenType::SlashAssign
                | TokenType::PercentAssign
        ) {
            return Err(self.error("Expected assignment operator after set target"));
        }

        let value = self.parse_expression()?;
        let declared_type = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let is_mutable = if self.check(TokenType::Mut) {
            self.advance();
            true
        } else {
            false
        };
        let is_constant = if self.check(TokenType::Const) {
            self.advance();
            true
        } else {
            false
        };

        if is_compound {
            let operator = match op.ttype {
                TokenType::PlusAssign => "+",
                TokenType::MinusAssign => "-",
                TokenType::StarAssign => "*",
                TokenType::SlashAssign => "/",
                TokenType::PercentAssign => "%",
                _ => unreachable!(),
            };
            let left = Expression::Identifier(Identifier {
                name: target.clone(),
                data_type: DataType::Unknown,
                line: 0,
                column: 0,
            });
            let expr = Expression::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(left),
                right: Box::new(value),
                data_type: DataType::Unknown,
            };
            return Ok(Statement::Assignment {
                target,
                value: expr,
                is_mutable: true,
            });
        }

        if target.contains('.') {
            return Ok(Statement::Assignment {
                target,
                value,
                is_mutable: true,
            });
        }

        let already_declared = self.is_declared(&target);
        if declared_type.is_none() && !is_constant && already_declared {
            return Ok(Statement::Assignment {
                target,
                value,
                is_mutable: true,
            });
        }

        let data_type = declared_type.unwrap_or(DataType::Unknown);
        self.declare(&target);
        Ok(Statement::Let {
            name: target,
            data_type,
            value: Some(value),
            is_constant,
            is_mutable,
            is_static: false,
            visibility: Visibility::Private,
        })
    }

    fn parse_visibility(&mut self) -> Result<Visibility> {
        if self.check(TokenType::Pub) {
            self.advance();
            Ok(Visibility::Public)
        } else if self.check(TokenType::Priv) {
            self.advance();
            Ok(Visibility::Private)
        } else {
            Err(self.error("Expected visibility keyword"))
        }
    }

    fn parse_fn_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.expect(TokenType::Fn)?;
        let name = self.expect_ident()?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::Lparen)?;
        let mut params = self.parse_param_list()?;
        self.expect(TokenType::Rparen)?;

        let return_type = if self.check(TokenType::Colon) {
            self.advance();
            self.parse_type()?
        } else {
            DataType::None
        };

        if self.method_context > 0 && !params.iter().any(|(name, _)| name == "self") {
            params.insert(0, ("self".to_string(), DataType::Unknown));
        }

        self.expect_block_open()?;
        self.push_scope();
        for (param_name, _) in &params {
            self.declare(param_name);
        }
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        self.declare(&name);

        Ok(Statement::Function {
            name,
            params,
            body,
            return_type,
            visibility,
            is_method: self.method_context > 0,
        })
    }

    fn parse_struct_statement(&mut self) -> Result<Statement> {
        if matches!(self.peek().ttype, TokenType::Pub | TokenType::Priv) {
            self.advance();
        }
        self.expect(TokenType::Struct)?;
        let name = self.expect_ident()?;

        let parent = if self.check(TokenType::Extends) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect_block_open()?;
        let mut fields = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            // Parse field as: name :type (without set)
            if self.peek().ttype == TokenType::Ident {
                let field_name = self.expect_ident()?;
                let field_type = if self.check(TokenType::Colon) {
                    self.advance();
                    self.parse_type()?
                } else {
                    DataType::Unknown
                };
                fields.push(Statement::Let {
                    name: field_name,
                    data_type: field_type,
                    value: None,
                    is_constant: false,
                    is_mutable: true,
                    is_static: false,
                    visibility: Visibility::Private,
                });
            }
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.declare(&name);
        Ok(Statement::Type {
            name,
            parent,
            fields,
        })
    }

    fn parse_type_statement(&mut self) -> Result<Statement> {
        self.parse_struct_statement()
    }

    fn parse_skill_statement(&mut self) -> Result<Statement> {
        if matches!(self.peek().ttype, TokenType::Pub | TokenType::Priv) {
            self.advance();
        }
        self.expect(TokenType::Skill)?;
        let name = self.expect_ident()?;
        self.expect_block_open()?;
        let mut methods = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            // Parse method signature: fn name: (params) :return_type
            self.expect(TokenType::Fn)?;
            let method_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            self.expect(TokenType::Lparen)?;
            let params = self.parse_param_list()?;
            self.expect(TokenType::Rparen)?;

            // Parse return type with :type (not =>)
            let return_type = if self.check(TokenType::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                DataType::None
            };

            methods.push(TraitMethodSig {
                name: method_name,
                params,
                return_type,
            });
            self.skip_newlines();
        }

        self.expect_block_close()?;
        Ok(Statement::Skill { name, methods })
    }

    fn parse_code_statement(&mut self) -> Result<Statement> {
        if matches!(self.peek().ttype, TokenType::Pub | TokenType::Priv) {
            self.advance();
        }
        self.expect(TokenType::Code)?;
        let trait_name = self.expect_ident()?;

        // Parse "to TypeName"
        self.expect(TokenType::To)?;
        let type_name = self.expect_ident()?;

        self.expect_block_open()?;
        let mut methods = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            // Parse method: fn name: (params) return_type > body <
            if self.check(TokenType::Fn) {
                let method_stmt = self.parse_fn_statement(Visibility::Private)?;
                methods.push(method_stmt);
            }
            self.skip_newlines();
        }

        self.expect_block_close()?;
        Ok(Statement::Code {
            trait_name,
            type_name,
            methods,
        })
    }

    fn parse_trait_statement(&mut self) -> Result<Statement> {
        if matches!(self.peek().ttype, TokenType::Pub | TokenType::Priv) {
            self.advance();
        }
        self.expect(TokenType::Trait)?;
        let name = self.expect_ident()?;
        self.expect_block_open()?;
        let mut methods = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            self.expect(TokenType::Fn)?;
            let method_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            self.expect(TokenType::Lparen)?;
            let mut params = self.parse_param_list()?;
            self.expect(TokenType::Rparen)?;
            if !params.iter().any(|(name, _)| name == "self") {
                params.insert(0, ("self".to_string(), DataType::Unknown));
            }
            let return_type = if self.check(TokenType::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                DataType::None
            };
            methods.push(TraitMethodSig {
                name: method_name,
                params,
                return_type,
            });
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.declare(&name);
        Ok(Statement::Trait { name, methods })
    }

    fn parse_impl_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Impl)?;
        let first = self.expect_ident()?;
        let (trait_name, type_name) = if self.check(TokenType::For) {
            self.advance();
            (Some(first), self.expect_ident()?)
        } else {
            (None, first)
        };

        self.expect_block_open()?;
        self.method_context += 1;
        let mut methods = Vec::new();
        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            methods.push(self.parse_statement()?);
            self.skip_newlines();
        }
        self.method_context = self.method_context.saturating_sub(1);
        self.expect_block_close()?;

        Ok(Statement::Impl {
            trait_name,
            type_name,
            methods,
        })
    }

    fn parse_enum_statement(&mut self) -> Result<Statement> {
        if matches!(self.peek().ttype, TokenType::Pub | TokenType::Priv) {
            self.advance();
        }
        self.expect(TokenType::Enum)?;
        let name = self.expect_ident()?;
        self.expect_block_open()?;
        let mut variants = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            let variant_name = self.expect_ident()?;
            let payload_types = if self.check(TokenType::Lparen) {
                self.advance();
                let mut types = Vec::new();
                while !self.check(TokenType::Rparen) && !self.is_at_end() {
                    let _binding = self.expect_ident()?;
                    self.expect(TokenType::Colon)?;
                    types.push(self.parse_type()?);
                }
                self.expect(TokenType::Rparen)?;
                types
            } else {
                Vec::new()
            };
            variants.push((variant_name, payload_types));
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.declare(&name);
        Ok(Statement::Enum { name, variants })
    }

    fn parse_if_statement(&mut self) -> Result<Statement> {
        let if_token = self.peek();
        self.expect(TokenType::If)?;
        let condition = self.parse_expression_until_block_open()?;

        // Check if we got a block open, if not report error at 'if' position
        if !self.check(TokenType::Lbrace) {
            return Err(self.error_at(
                if_token.line,
                if_token.column,
                "Expected '{' after if condition",
            ));
        }
        self.expect_block_open()?;

        self.push_scope();
        let then_branch = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;

        let else_branch = if self.check(TokenType::Elif) {
            let nested = self.parse_if_statement_from_elif()?;
            Some(vec![nested])
        } else if self.check(TokenType::Else) {
            self.advance();
            self.expect_block_open()?;
            self.push_scope();
            let body = self.parse_block()?;
            self.pop_scope();
            self.expect_block_close()?;
            Some(body)
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_if_statement_from_elif(&mut self) -> Result<Statement> {
        self.expect(TokenType::Elif)?;
        let condition = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        self.push_scope();
        let then_branch = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;

        let else_branch = if self.check(TokenType::Elif) {
            Some(vec![self.parse_if_statement_from_elif()?])
        } else if self.check(TokenType::Else) {
            self.advance();
            self.expect_block_open()?;
            self.push_scope();
            let body = self.parse_block()?;
            self.pop_scope();
            self.expect_block_close()?;
            Some(body)
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::While)?;
        let condition = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        self.push_scope();
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        Ok(Statement::While { condition, body })
    }

    fn parse_for_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::For)?;
        let first = self.expect_ident()?;
        let second = if self.check(TokenType::Comma) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect(TokenType::In)?;
        let iterable = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        self.push_scope();
        self.declare(&first);
        if let Some(second) = &second {
            self.declare(second);
        }
        let mut body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;

        if let Some(second) = second {
            body.insert(
                0,
                Statement::Let {
                    name: second,
                    data_type: DataType::Anything,
                    value: None,
                    is_constant: false,
                    is_mutable: true,
                    is_static: false,
                    visibility: Visibility::Private,
                },
            );
        }

        Ok(Statement::For {
            variable: first,
            iterable,
            body,
        })
    }

    fn parse_do_while_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Do)?;
        self.expect_block_open()?;
        self.push_scope();
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        self.expect(TokenType::While)?;
        let condition = self.parse_expression()?;

        Ok(Statement::Expression(Expression::Call {
            name: "__do_while".to_string(),
            args: vec![
                Expression::Closure {
                    params: Vec::new(),
                    body,
                    return_type: DataType::None,
                    capture: Vec::new(),
                },
                Expression::Closure {
                    params: Vec::new(),
                    body: vec![Statement::Return(Some(condition))],
                    return_type: DataType::Bool,
                    capture: Vec::new(),
                },
            ],
            data_type: DataType::None,
        }))
    }

    fn parse_match_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Match)?;

        let value = self.parse_match_value()?;

        // Expect opening brace
        self.expect(TokenType::Lbrace)?;

        let mut cases = Vec::new();
        let mut default = Vec::new();

        loop {
            self.skip_newlines();

            // Stop at final block close (})
            if self.check(TokenType::Rbrace) {
                self.advance(); // Consume final }
                break;
            }

            if !self.next_tokens_form_match_case() {
                break;
            }

            let pattern = self.parse_match_pattern()?;

            // Skip newlines after pattern
            self.skip_newlines();

            self.expect(TokenType::Lbrace)?;
            self.skip_newlines();
            let body = self.parse_statements_until_block_close()?;
            self.expect(TokenType::Rbrace)?;

            let is_default = matches!(
                &pattern,
                Expression::Identifier(Identifier { name, .. }) if name == "_"
            );
            if is_default {
                default = body;
            } else {
                cases.push((pattern, body));
            }
        }

        Ok(Statement::Match {
            value,
            cases,
            default,
        })
    }

    fn make_expression_statement(&self, expr: Expression) -> Result<Statement> {
        Ok(Statement::Expression(expr))
    }

    fn parse_return_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Return)?;
        if self.is_statement_terminator() {
            return Ok(Statement::Return(None));
        }
        let expr = self.parse_expression()?;
        Ok(Statement::Return(Some(expr)))
    }

    fn parse_block(&mut self) -> Result<Vec<Statement>> {
        let mut statements = Vec::new();
        loop {
            self.skip_newlines();
            if self.check_block_close()
                || self.check(TokenType::Else)
                || self.check(TokenType::Elif)
                || self.is_at_end()
            {
                break;
            }
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }
        Ok(statements)
    }

    fn parse_expression(&mut self) -> Result<Expression> {
        self.parse_pipeline_free_expression()
    }

    fn parse_pipeline_free_expression(&mut self) -> Result<Expression> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression> {
        let mut expr = self.parse_and()?;
        while self.check(TokenType::Or) {
            self.advance();
            let right = self.parse_and()?;
            expr = Expression::BinaryOp {
                operator: "or".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Bool,
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expression> {
        let mut expr = self.parse_equality()?;
        while self.check(TokenType::And) {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expression::BinaryOp {
                operator: "and".to_string(),
                left: Box::new(expr),
                right: Box::new(right),
                data_type: DataType::Bool,
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression> {
        let mut expr = self.parse_comparison()?;
        loop {
            if self.check(TokenType::Eq) {
                self.advance();
                let right = self.parse_comparison()?;
                expr = Expression::BinaryOp {
                    operator: "==".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Neq) {
                self.advance();
                let right = self.parse_comparison()?;
                expr = Expression::BinaryOp {
                    operator: "!=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Is) {
                self.advance();
                self.expect(TokenType::Lparen)?;
                let right = self.parse_expression()?;
                self.expect(TokenType::Rparen)?;
                expr = Expression::Call {
                    name: "__is".to_string(),
                    args: vec![expr, right],
                    data_type: DataType::Bool,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression> {
        let mut expr = self.parse_additive()?;

        loop {
            if self.check(TokenType::Pipeline) || self.check(TokenType::PipelineSafe) {
                let is_safe = self.check(TokenType::PipelineSafe);
                if self.check(TokenType::PipelineSafe) {
                    self.advance();
                } else {
                    self.advance();
                }
                let stage = self.parse_additive()?;
                expr = self.apply_pipeline(expr, stage, is_safe)?;
                continue;
            }

            if self.check(TokenType::Gt) {
                self.advance();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: ">".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Lt) {
                self.advance();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: "<".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Gte) {
                self.advance();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: ">=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::Lte) {
                self.advance();
                let right = self.parse_additive()?;
                expr = Expression::BinaryOp {
                    operator: "<=".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::In) {
                self.advance();
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
                    data_type: DataType::Bool,
                };
            } else if self.check(TokenType::At) {
                self.advance();
                let index = self.parse_additive()?;
                expr = Expression::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::To) {
                self.advance();
                let right = self.parse_additive()?;
                expr = Expression::Call {
                    name: "range".to_string(),
                    args: vec![expr, right],
                    data_type: DataType::List,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expression> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            if self.check(TokenType::Plus) {
                self.advance();
                let right = self.parse_multiplicative()?;
                expr = Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Minus) {
                self.advance();
                let right = self.parse_multiplicative()?;
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

    fn parse_multiplicative(&mut self) -> Result<Expression> {
        let mut expr = self.parse_unary()?;
        loop {
            if self.check(TokenType::Star) {
                self.advance();
                let right = self.parse_unary()?;
                expr = Expression::BinaryOp {
                    operator: "*".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Slash) {
                self.advance();
                let right = self.parse_unary()?;
                expr = Expression::BinaryOp {
                    operator: "/".to_string(),
                    left: Box::new(expr),
                    right: Box::new(right),
                    data_type: DataType::Unknown,
                };
            } else if self.check(TokenType::Percent) {
                self.advance();
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

        if self.check(TokenType::Not) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expression::UnaryOp {
                operator: "not".to_string(),
                operand: Box::new(operand),
                data_type: DataType::Bool,
            });
        }

        if self.check(TokenType::Amp) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expression::Reference {
                expr: Box::new(expr),
                is_mutable: false,
                data_type: DataType::Ref,
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

            if self.check(TokenType::Lparen) {
                let call_target = match &expr {
                    Expression::Identifier(Identifier { name, .. }) => Some(name.clone()),
                    Expression::MemberAccess { target, member, .. } => {
                        let module = match &**target {
                            Expression::Identifier(Identifier { name, .. }) => Some(name.clone()),
                            _ => None,
                        };
                        module.map(|m| format!("{}.{}", m, member))
                    }
                    _ => None,
                };
                if let Some(name) = call_target {
                    if matches!(name.as_str(), "dasu" | "ireru" | "std.output" | "std.input") {
                        expr = self.parse_template_call(name)?;
                    } else {
                        let args = self.parse_call_arguments()?;
                        expr = Expression::Call {
                            name,
                            args,
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
                        data_type: DataType::Unknown,
                    };
                }
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression> {
        match self.peek().ttype {
            TokenType::Use => self.parse_use_expression(),
            TokenType::If => self.parse_if_expression(),
            TokenType::Match => {
                self.advance();
                self.parse_match_expression()
            }
            TokenType::IntLit => {
                let value = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Int(
                    value.parse().unwrap_or(0),
                )))
            }
            TokenType::FloatLit => {
                let value = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Float(
                    value.parse().unwrap_or(0.0),
                )))
            }
            TokenType::StrLit => {
                let value = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Str(value)))
            }
            TokenType::BoolLit => {
                let value = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Bool(value == "true")))
            }
            TokenType::NoneLit => {
                self.advance();
                Ok(Expression::Literal(Literal::None))
            }
            TokenType::SelfToken => {
                self.advance();
                Ok(identifier_expr("self"))
            }
            TokenType::Ident => {
                let name = self.advance().value.unwrap_or_default();
                if name == "type" && self.is_expression_start(self.peek().ttype) {
                    let expr = self.parse_expression()?;
                    return Ok(Expression::Call {
                        name: "type".to_string(),
                        args: vec![expr],
                        data_type: DataType::Str,
                    });
                }
                Ok(identifier_expr(&name))
            }
            TokenType::Lparen => {
                self.advance();

                // Check if this is a struct construction: (TypeName field:value ...)
                // Pattern: (Ident Ident: value ...)
                if self.check(TokenType::Ident) {
                    let first_token = self.peek();
                    let type_name = first_token.value.clone().unwrap_or_default();

                    // Look ahead: we need Ident followed by Colon
                    if self.peek_n(1).ttype == TokenType::Ident
                        && self.peek_n(2).ttype == TokenType::Colon
                    {
                        self.advance(); // consume type name

                        let mut args = Vec::new();

                        // Parse field:value pairs
                        while !self.check(TokenType::Rparen) && !self.is_at_end() {
                            if self.check(TokenType::Ident)
                                && self.peek_n(1).ttype == TokenType::Colon
                            {
                                let field_name = self.advance().value.clone().unwrap_or_default();
                                self.advance(); // consume colon
                                let value_expr = self.parse_expression()?;
                                args.push(Expression::NamedArg {
                                    name: field_name,
                                    value: Box::new(value_expr),
                                    data_type: DataType::Unknown,
                                });

                                // Skip comma if present
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
                            data_type: DataType::Unknown,
                        });
                    }
                }

                // Regular parenthesized expression
                let expr = self.parse_expression()?;
                self.expect(TokenType::Rparen)?;
                Ok(expr)
            }
            TokenType::Lbracket => self.parse_bracket_literal(),
            TokenType::Lbrace => self.parse_brace_literal(),
            _ => Err(self.error("Unexpected token in expression")),
        }
    }

    fn parse_use_expression(&mut self) -> Result<Expression> {
        self.expect(TokenType::Use)?;
        let expr = self.parse_pipeline_free_expression()?;

        // If expr is just an identifier (function name), convert to call with empty args
        let result = if let Expression::Identifier(ident) = expr {
            Expression::Call {
                name: ident.name.clone(),
                args: Vec::new(),
                data_type: DataType::Unknown,
            }
        } else if let Expression::Call { name: _, args, .. } = &expr {
            // If it's already a Call but has no args, ensure it's treated as function call
            // Check if this was parsed as identifier-only call, fix args
            if args.is_empty() {
                // Check what the next token is - if Lparen, args were already parsed
                expr
            } else {
                expr
            }
        } else {
            expr
        };

        let mut final_expr = result;
        while self.check(TokenType::Pipeline) || self.check(TokenType::PipelineSafe) {
            let is_safe = self.check(TokenType::PipelineSafe);
            if self.check(TokenType::PipelineSafe) {
                self.advance();
            } else {
                self.advance();
            }
            let stage = self.parse_pipeline_free_expression()?;
            final_expr = self.apply_pipeline(final_expr, stage, is_safe)?;
        }
        Ok(final_expr)
    }

    fn parse_if_expression(&mut self) -> Result<Expression> {
        self.expect(TokenType::If)?;
        let condition = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        let then_parsed = self.parse_expression_until_block_close()?;
        let then_expr = self.coerce_unknown_identifier_to_string(then_parsed);
        self.expect_block_close()?;
        self.expect(TokenType::Else)?;
        self.expect_block_open()?;
        let else_parsed = self.parse_expression_until_block_close()?;
        let else_expr = self.coerce_unknown_identifier_to_string(else_parsed);
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
            data_type: DataType::Unknown,
        })
    }

    fn parse_match_expression(&mut self) -> Result<Expression> {
        // Parse the match value
        let value = self.parse_match_value()?;

        // Skip newlines before {
        self.skip_newlines();

        // Expect opening brace
        self.expect(TokenType::Lbrace)?;
        self.skip_newlines();

        let mut cases = Vec::new();
        let mut default = None;

        loop {
            self.skip_newlines();

            // Stop at final }
            if self.check(TokenType::Rbrace) {
                self.advance(); // Consume final }
                break;
            }

            if !self.next_tokens_form_match_case() {
                break;
            }

            let pattern_expr = self.parse_match_pattern()?;

            // Skip newlines after pattern
            self.skip_newlines();

            self.expect(TokenType::Lbrace)?;
            self.skip_newlines();

            let body_expr = self.parse_expression_until_block_close()?;
            self.expect(TokenType::Rbrace)?;

            let is_default = matches!(
                &pattern_expr,
                Expression::Identifier(Identifier { name, .. }) if name == "_"
            );
            if is_default {
                default = Some(body_expr);
            } else {
                cases.push((pattern_expr, body_expr));
            }

            self.skip_newlines();
        }

        // Consume final } if present (already consumed in loop, but just in case)
        self.skip_newlines();

        Ok(Expression::Match {
            value: Box::new(value),
            cases,
            default: Box::new(default.unwrap_or(Expression::Literal(Literal::None))),
            data_type: DataType::Unknown,
        })
    }

    fn parse_value_until_block_open(&mut self) -> Result<Expression> {
        // Skip whitespace and newlines to find start
        while self.pos < self.tokens.len() {
            let tt = self.peek().ttype;
            if tt == TokenType::Newline || tt == TokenType::Comma {
                self.advance();
            } else {
                break;
            }
        }

        let start = self.pos;
        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;

        while self.pos < self.tokens.len() {
            let tt = self.peek().ttype;
            if tt == TokenType::Lparen {
                depth_paren += 1;
            } else if tt == TokenType::Rparen {
                depth_paren = depth_paren.saturating_sub(1);
            } else if tt == TokenType::Lbracket {
                depth_bracket += 1;
            } else if tt == TokenType::Rbracket {
                depth_bracket = depth_bracket.saturating_sub(1);
            } else if tt == TokenType::Lbrace && depth_paren == 0 && depth_bracket == 0 {
                break;
            }
            self.advance();
        }

        let end = self.pos;

        // Reset position to start
        self.pos = start;

        if self.pos >= end {
            return Err(self.error("Expected value in match expression"));
        }

        let token = self.peek();

        match token.ttype {
            TokenType::Ident => {
                let name = self.advance().value.unwrap_or_default();
                Ok(identifier_expr(&name))
            }
            TokenType::IntLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Int(val.parse().unwrap_or(0))))
            }
            TokenType::FloatLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Float(
                    val.parse().unwrap_or(0.0),
                )))
            }
            TokenType::StrLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Str(val)))
            }
            _ => Err(self.error("Expected identifier or literal in match value")),
        }
    }

    fn parse_match_pattern(&mut self) -> Result<Expression> {
        let token = self.peek();

        match token.ttype {
            TokenType::Ident => {
                let name = self.advance().value.unwrap_or_default();
                if name == "_" {
                    Ok(identifier_expr("_"))
                } else {
                    Ok(identifier_expr(&name))
                }
            }
            TokenType::IntLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Int(val.parse().unwrap_or(0))))
            }
            TokenType::FloatLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Float(
                    val.parse().unwrap_or(0.0),
                )))
            }
            TokenType::StrLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Str(val)))
            }
            _ => Err(self.error("Expected pattern in match case")),
        }
    }

    fn parse_match_value(&mut self) -> Result<Expression> {
        // Skip whitespace and newlines ONLY at the start
        while self.peek().ttype == TokenType::Newline {
            self.advance();
        }

        let token = self.peek();

        // Handle match expression syntax: value is just a simple identifier or literal
        // (not advance until > like before)
        match token.ttype {
            TokenType::Ident => {
                let name = self.advance().value.unwrap_or_default();
                Ok(identifier_expr(&name))
            }
            TokenType::IntLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Int(val.parse().unwrap_or(0))))
            }
            TokenType::FloatLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Float(
                    val.parse().unwrap_or(0.0),
                )))
            }
            TokenType::StrLit => {
                let val = self.advance().value.unwrap_or_default();
                Ok(Expression::Literal(Literal::Str(val)))
            }
            TokenType::Newline => {
                // Skip newlines and try again
                self.advance();
                self.parse_match_value()
            }
            _ => Err(self.error("Expected value in match expression")),
        }
    }

    fn parse_expression_until_block_open(&mut self) -> Result<Expression> {
        let start = self.pos;
        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;

        while !self.is_at_end() {
            match self.peek().ttype {
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                TokenType::Lbrace if depth_paren == 0 && depth_bracket == 0 => {
                    break;
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
        let mut parser = Parser::new(slice);
        parser.scopes = self.scopes.clone();
        let expr = parser.parse_expression()?;
        Ok(expr)
    }

    fn parse_expression_until_block_close(&mut self) -> Result<Expression> {
        let start = self.pos;
        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;

        while !self.is_at_end() {
            match self.peek().ttype {
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                TokenType::Rbrace if depth_paren == 0 && depth_bracket == 0 => {
                    break;
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
        let mut parser = Parser::new(slice);
        parser.scopes = self.scopes.clone();
        let expr = parser.parse_expression()?;
        Ok(expr)
    }

    fn parse_statements_until_block_close(&mut self) -> Result<Vec<Statement>> {
        let start = self.pos;
        let mut depth_paren = 0usize;
        let mut depth_bracket = 0usize;

        while !self.is_at_end() {
            match self.peek().ttype {
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                TokenType::Rbrace if depth_paren == 0 && depth_bracket == 0 => {
                    break;
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

        let mut parser = Parser::new(slice);
        parser.scopes = self.scopes.clone();
        parser.push_scope();
        Ok(parser.parse()?.statements)
    }

    fn parse_call_arguments(&mut self) -> Result<Vec<Expression>> {
        self.expect(TokenType::Lparen)?;
        let mut args = Vec::new();
        while !self.check(TokenType::Rparen) && !self.is_at_end() {
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
        self.expect(TokenType::Rparen)?;
        Ok(args)
    }

    fn parse_template_call(&mut self, name: String) -> Result<Expression> {
        self.expect(TokenType::Lparen)?;
        let template = self.parse_template_expression()?;
        self.expect(TokenType::Rparen)?;

        // Parse optional type annotation: ireru(prompt) :i64
        let data_type = if self.check(TokenType::Colon) {
            self.advance();
            self.parse_type()?
        } else {
            DataType::Str // default to str
        };

        Ok(Expression::Call {
            name,
            args: vec![template],
            data_type,
        })
    }

    fn parse_template_expression(&mut self) -> Result<Expression> {
        let mut current_text = String::new();
        let mut parts: Vec<Expression> = Vec::new();

        while !self.check(TokenType::Rparen) && !self.is_at_end() {
            if self.check(TokenType::Lbrace) {
                self.advance();
                if current_text
                    .chars()
                    .last()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == ')' || ch == ']')
                {
                    current_text.push(' ');
                }
                if !current_text.is_empty() {
                    parts.push(string_expr(&current_text));
                    current_text.clear();
                }
                let interpolation = self.parse_interpolation_expression()?;
                parts.push(interpolation);
                self.expect(TokenType::Rbrace)?;
                continue;
            }

            let token = self.advance();
            if token.ttype == TokenType::StrLit {
                let value = token.value.clone().unwrap_or_default();
                if value.contains('{') {
                    if !current_text.is_empty() {
                        parts.push(string_expr(&current_text));
                        current_text.clear();
                    }
                    parts.extend(self.parse_string_template_parts(&value)?);
                    continue;
                }
            }
            push_template_text(&mut current_text, &token);
        }

        if !current_text.is_empty() {
            parts.push(string_expr(&current_text));
        }

        Ok(concat_expressions(parts))
    }

    fn parse_interpolation_expression(&mut self) -> Result<Expression> {
        let expr = self.parse_expression()?;
        if self.check(TokenType::Colon) {
            self.advance();
            let mut spec = String::new();
            while !self.check(TokenType::Rbrace) && !self.is_at_end() {
                let token = self.advance();
                spec.push_str(&self.token_surface(token));
            }
            return Ok(Expression::Call {
                name: "__mire_fmt".to_string(),
                args: vec![expr, string_expr(&spec)],
                data_type: DataType::Str,
            });
        }

        Ok(Expression::Call {
            name: "str".to_string(),
            args: vec![expr],
            data_type: DataType::Str,
        })
    }

    fn parse_string_template_parts(&self, value: &str) -> Result<Vec<Expression>> {
        let mut parts = Vec::new();
        let mut text = String::new();
        let chars: Vec<char> = value.chars().collect();
        let mut index = 0usize;

        while index < chars.len() {
            match chars[index] {
                '{' if index + 1 < chars.len() && chars[index + 1] == '{' => {
                    text.push('{');
                    index += 2;
                }
                '}' if index + 1 < chars.len() && chars[index + 1] == '}' => {
                    text.push('}');
                    index += 2;
                }
                '{' => {
                    if !text.is_empty() {
                        parts.push(string_expr(&text));
                        text.clear();
                    }

                    let start = index + 1;
                    let mut depth = 1usize;
                    index += 1;
                    while index < chars.len() && depth > 0 {
                        match chars[index] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        index += 1;
                    }

                    if depth != 0 {
                        return Err(self.error("Unclosed interpolation in template string"));
                    }

                    let inner: String = chars[start..index - 1].iter().collect();
                    let interpolation = self.parse_interpolation_source(&inner)?;
                    parts.push(interpolation);
                }
                ch => {
                    text.push(ch);
                    index += 1;
                }
            }
        }

        if !text.is_empty() {
            parts.push(string_expr(&text));
        }

        Ok(parts)
    }

    fn parse_interpolation_source(&self, source: &str) -> Result<Expression> {
        let mut parser = Parser::new(tokenize(source)?);
        parser.scopes = self.scopes.clone();

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
            data_type: DataType::Str,
        })
    }

    fn parse_param_list(&mut self) -> Result<Vec<(String, DataType)>> {
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
                self.parse_type()?
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

    fn parse_type(&mut self) -> Result<DataType> {
        if self.check(TokenType::Amp) {
            self.advance();
            let _ = self.parse_type()?;
            return Ok(DataType::Ref);
        }

        if self.check(TokenType::NoneLit) {
            self.advance();
            return Ok(DataType::None);
        }

        if self.check(TokenType::Ident) {
            let name = self.advance().value.unwrap_or_default();
            return match name.as_str() {
                "i8" => Ok(DataType::I8),
                "i16" => Ok(DataType::I16),
                "i32" => Ok(DataType::I32),
                "i64" => Ok(DataType::I64),
                "u8" => Ok(DataType::U8),
                "u16" => Ok(DataType::U16),
                "u32" => Ok(DataType::U32),
                "u64" => Ok(DataType::U64),
                "f32" => Ok(DataType::F32),
                "f64" => Ok(DataType::F64),
                "str" => Ok(DataType::Str),
                "bool" => Ok(DataType::Bool),
                "none" | "mu" => Ok(DataType::None),
                "arr" => {
                    self.expect(TokenType::Lbracket)?;
                    let element_type = Box::new(self.parse_type()?);
                    let size = self.expect_int_literal()?.parse().unwrap_or(0);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Array { element_type, size })
                }
                "vec" => {
                    let dynamic = if self.check(TokenType::Bang) {
                        self.advance();
                        true
                    } else {
                        false
                    };
                    self.expect(TokenType::Lbracket)?;
                    let element_type = Box::new(self.parse_type()?);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Vector {
                        element_type,
                        dynamic,
                    })
                }
                "map" => {
                    self.expect(TokenType::Lbracket)?;
                    let key_type = Box::new(self.parse_type()?);
                    let value_type = Box::new(self.parse_type()?);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Map {
                        key_type,
                        value_type,
                    })
                }
                other => {
                    if self.check(TokenType::Bang) {
                        self.advance();
                        self.expect(TokenType::Lbracket)?;
                        let element_type = Box::new(self.parse_type()?);
                        self.expect(TokenType::Rbracket)?;
                        Ok(DataType::Vector {
                            element_type,
                            dynamic: true,
                        })
                    } else {
                        Ok(DataType::from_str(other))
                    }
                }
            };
        }

        Err(self.error("Expected type"))
    }

    fn parse_type_name_string(&mut self) -> Result<String> {
        let start = self.pos;
        let _ = self.parse_type()?;
        let mut out = String::new();
        for token in &self.tokens[start..self.pos] {
            out.push_str(&self.token_surface(token.clone()));
        }
        Ok(out)
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
                let parsed_key = self.parse_expression()?;
                let key = self.coerce_unknown_identifier_to_string(parsed_key);
                let value = self.parse_expression()?;
                entries.push((key, value));
                if self.check(TokenType::Comma) {
                    self.advance();
                }
            }
            self.expect(TokenType::Rbracket)?;
            Ok(Expression::Dict {
                entries,
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
            let parsed_key = self.parse_expression()?;
            let key = self.coerce_unknown_identifier_to_string(parsed_key);
            self.expect(TokenType::Colon)?;
            let value = self.parse_expression()?;
            entries.push((key, value));

            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
        }

        self.expect(TokenType::Rbrace)?;
        Ok(Expression::Dict {
            entries,
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

    fn parse_assignment_target(&mut self) -> Result<String> {
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

        Ok(target)
    }

    fn expect_ident(&mut self) -> Result<String> {
        if self.check(TokenType::Ident) {
            Ok(self.advance().value.unwrap_or_default())
        } else {
            Err(self.error("Expected identifier"))
        }
    }

    fn expect_member_name(&mut self) -> Result<String> {
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

    fn expect_int_literal(&mut self) -> Result<String> {
        if self.check(TokenType::IntLit) {
            Ok(self.advance().value.unwrap_or_default())
        } else {
            Err(self.error("Expected integer literal"))
        }
    }

    fn expect_block_close(&mut self) -> Result<()> {
        if self.check(TokenType::Rbrace) || self.is_at_end() {
            self.advance();
            Ok(())
        } else {
            Err(self.error("Expected '}' to close a block"))
        }
    }

    fn expect_block_open(&mut self) -> Result<()> {
        if self.check(TokenType::Lbrace) {
            self.advance();
            Ok(())
        } else {
            Err(self.error("Expected '{' to start a block"))
        }
    }

    fn check_block_close(&self) -> bool {
        self.check(TokenType::Rbrace)
    }

    fn expect(&mut self, token_type: TokenType) -> Result<()> {
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

    fn error_at(&self, line: usize, column: usize, message: &str) -> MireError {
        MireError::new(ErrorKind::Parser {
            line,
            column,
            message: message.to_string(),
        })
    }

    fn error(&self, message: &str) -> MireError {
        let token = self.peek();
        self.error_at(token.line, token.column, message)
    }

    fn bracket_contains_top_level_comma(&self) -> bool {
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

    fn is_expression_start(&self, token: TokenType) -> bool {
        matches!(
            token,
            TokenType::Ident
                | TokenType::IntLit
                | TokenType::FloatLit
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
                | TokenType::Not
                | TokenType::Amp
                | TokenType::Star
        )
    }

    fn next_tokens_form_match_case(&self) -> bool {
        let index = self.pos;

        // Skip leading newlines
        let mut i = index;
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
                // These are match-ending tokens - they terminate the match
                TokenType::Rbrace if depth_paren == 0 && depth_bracket == 0 => {
                    return false;
                }
                // These are match-ending tokens for expressions
                TokenType::Colon | TokenType::Eof if depth_paren == 0 && depth_bracket == 0 => {
                    return false;
                }
                // Newlines don't terminate a match case - they're just separators
                TokenType::Newline => {
                    index += 1;
                    continue;
                }
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                // Also return true for literals that can start a pattern
                TokenType::IntLit
                | TokenType::FloatLit
                | TokenType::StrLit
                | TokenType::BoolLit
                | TokenType::NoneLit
                    if depth_paren == 0 && depth_bracket == 0 =>
                {
                    return true;
                }
                // Newlines don't terminate a match case - they're just separators
                TokenType::Newline => {
                    index += 1;
                    continue;
                }
                TokenType::Lparen => depth_paren += 1,
                TokenType::Rparen => depth_paren = depth_paren.saturating_sub(1),
                TokenType::Lbracket => depth_bracket += 1,
                TokenType::Rbracket => depth_bracket = depth_bracket.saturating_sub(1),
                // Also return true for literals that can start a pattern
                TokenType::IntLit
                | TokenType::FloatLit
                | TokenType::StrLit
                | TokenType::BoolLit
                | TokenType::NoneLit
                    if depth_paren == 0 && depth_bracket == 0 =>
                {
                    return true;
                }
                // Also return true for identifiers (including _ for default)
                TokenType::Ident if depth_paren == 0 && depth_bracket == 0 => {
                    return true;
                }
                _ => {}
            }
            index += 1;
        }

        false
    }

    fn is_statement_terminator(&self) -> bool {
        matches!(
            self.peek().ttype,
            TokenType::Newline
                | TokenType::Rbrace
                | TokenType::Else
                | TokenType::Elif
                | TokenType::Eof
        )
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) {
            self.advance();
        }
    }

    fn check(&self, token_type: TokenType) -> bool {
        !self.is_at_end() && self.peek().ttype == token_type
    }

    fn peek(&self) -> Token {
        self.tokens
            .get(self.pos)
            .cloned()
            .unwrap_or(Token::new(TokenType::Eof, 0, 0))
    }

    fn peek_n(&self, n: usize) -> Token {
        self.tokens
            .get(self.pos + n)
            .cloned()
            .unwrap_or(Token::new(TokenType::Eof, 0, 0))
    }

    fn advance(&mut self) -> Token {
        let token = self.peek();
        if !self.is_at_end() {
            self.pos += 1;
        }
        token
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.peek().ttype == TokenType::Eof
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn token_surface(&self, token: Token) -> String {
        match token.ttype {
            TokenType::Ident
            | TokenType::IntLit
            | TokenType::FloatLit
            | TokenType::StrLit
            | TokenType::BoolLit => token.value.unwrap_or_default(),
            TokenType::NoneLit => "none".to_string(),
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

    fn coerce_unknown_identifier_to_string(&self, expr: Expression) -> Expression {
        match expr {
            Expression::Identifier(Identifier { name, .. }) if !self.is_declared(&name) => {
                string_expr(&name)
            }
            other => other,
        }
    }
}

fn identifier_expr(name: &str) -> Expression {
    Expression::Identifier(Identifier {
        name: name.to_string(),
        data_type: DataType::Unknown,
        line: 0,
        column: 0,
    })
}

fn string_expr(value: &str) -> Expression {
    Expression::Literal(Literal::Str(value.to_string()))
}

fn concat_expressions(mut parts: Vec<Expression>) -> Expression {
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

fn push_template_text(buf: &mut String, token: &Token) {
    let surface = match token.ttype {
        TokenType::Ident | TokenType::IntLit | TokenType::FloatLit | TokenType::BoolLit => {
            token.value.clone().unwrap_or_default()
        }
        TokenType::NoneLit => "none".to_string(),
        TokenType::StrLit => token.value.clone().unwrap_or_default(),
        TokenType::Comma => ",".to_string(),
        TokenType::Colon => ":".to_string(),
        TokenType::Dot => ".".to_string(),
        TokenType::Eq => "==".to_string(),
        TokenType::Assign => "=".to_string(),
        TokenType::Neq => "!=".to_string(),
        TokenType::Gt => ">".to_string(),
        TokenType::Lt => "<".to_string(),
        TokenType::Gte => ">=".to_string(),
        TokenType::Lte => "<=".to_string(),
        TokenType::Plus => "+".to_string(),
        TokenType::Minus => "-".to_string(),
        TokenType::Star => "*".to_string(),
        TokenType::Slash => "/".to_string(),
        TokenType::Percent => "%".to_string(),
        TokenType::Amp => "&".to_string(),
        TokenType::Bang => "!".to_string(),
        TokenType::Lparen => "(".to_string(),
        TokenType::Rparen => ")".to_string(),
        TokenType::Lbracket => "[".to_string(),
        TokenType::Rbracket => "]".to_string(),
        TokenType::Lbrace => "{".to_string(),
        TokenType::Rbrace => "}".to_string(),
        TokenType::Pipeline => "=>".to_string(),
        TokenType::PipelineSafe => "=>?".to_string(),
        TokenType::At => token.value.clone().unwrap_or_else(|| "at".to_string()),
        TokenType::Question => "?".to_string(),
        TokenType::Newline => "\n".to_string(),
        _ => token.value.clone().unwrap_or_default(),
    };

    if template_needs_space(buf, token.ttype) {
        buf.push(' ');
    }

    buf.push_str(&surface);
}

fn template_needs_space(buf: &str, token_type: TokenType) -> bool {
    let Some(prev) = buf.chars().last() else {
        return false;
    };

    if matches!(prev, ' ' | '\n' | '(' | '[' | '{') {
        return false;
    }

    match token_type {
        TokenType::Comma
        | TokenType::Dot
        | TokenType::Colon
        | TokenType::Rparen
        | TokenType::Rbracket
        | TokenType::Rbrace => false,
        TokenType::Question | TokenType::Bang => {
            !(prev.is_alphanumeric() || prev == '_' || matches!(prev, ')' | ']' | '}'))
        }
        TokenType::Newline => false,
        _ => true,
    }
}

fn is_word_surface(surface: &str) -> bool {
    surface
        .chars()
        .next()
        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}

fn contains_self_placeholder(expr: &Expression) -> bool {
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
    }
}

fn statement_contains_self_placeholder(statement: &Statement) -> bool {
    match statement {
        Statement::Let { value, .. } => value.as_ref().is_some_and(contains_self_placeholder),
        Statement::Assignment { value, .. } => contains_self_placeholder(value),
        Statement::Function { body, .. }
        | Statement::Unsafe { body }
        | Statement::Module { body, .. }
        | Statement::DmireTable { body, .. }
        | Statement::DmireColumn { body, .. } => {
            body.iter().any(statement_contains_self_placeholder)
        }
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
        Statement::Class { methods, .. }
        | Statement::Impl { methods, .. }
        | Statement::Code { methods, .. } => {
            methods.iter().any(statement_contains_self_placeholder)
        }
        Statement::Type { fields, .. } => fields.iter().any(statement_contains_self_placeholder),
        Statement::Skill { .. } => false,
        Statement::Asm { instructions } => instructions
            .iter()
            .any(|(_, expr)| contains_self_placeholder(expr)),
        Statement::Drop { value } => contains_self_placeholder(value),
        Statement::Move { value, .. } => contains_self_placeholder(value),
        Statement::DmireDlist { data, .. } => data.iter().any(contains_self_placeholder),
        Statement::Query { .. }
        | Statement::Break
        | Statement::Continue
        | Statement::Trait { .. }
        | Statement::Type { .. }
        | Statement::Skill { .. }
        | Statement::Code { .. }
        | Statement::ExternLib { .. }
        | Statement::ExternFunction { .. }
        | Statement::AddLib { .. }
        | Statement::Use { .. }
        | Statement::Enum { .. } => false,
    }
}

fn replace_self_placeholder(expr: Expression, replacement: &Expression) -> Expression {
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
            data_type,
        } => Expression::Call {
            name,
            args: args
                .into_iter()
                .map(|arg| replace_self_placeholder(arg, replacement))
                .collect(),
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
        Expression::Dict { entries, data_type } => Expression::Dict {
            entries: entries
                .into_iter()
                .map(|(key, value)| {
                    (
                        replace_self_placeholder(key, replacement),
                        replace_self_placeholder(value, replacement),
                    )
                })
                .collect(),
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
        } => Expression::Reference {
            expr: Box::new(replace_self_placeholder(*expr, replacement)),
            is_mutable,
            data_type,
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
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::parser::ast::{Expression, Literal, Statement};

    #[test]
    fn parses_dynamic_vector_type_annotation() {
        let source = "set xs = [] :vec![i64]\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn parses_inline_nested_vector_literal() {
        let source = "set xs = [[1 2] [3 4]] :vec[vec[i64]]\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn keeps_unknown_identifier_as_identifier_in_regular_call_arguments() {
        let source = "pub fn main: () {\nuse len(missing)\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Expression(Expression::Call { args, .. }) = &body[0] else {
            panic!("expected call expression");
        };
        assert!(matches!(args.first(), Some(Expression::Identifier(_))));
    }

    #[test]
    fn parses_import_and_brace_blocks() {
        let source = "import std\npub fn main: () {\n    use dasu(ok)\n}\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn parses_local_import_with_dot_slash() {
        let source = "import ./utils/helpers\n";
        let program = parse(source);
        assert!(program.is_ok(), "{program:?}");
    }

    #[test]
    fn parses_brace_map_literal() {
        let source = "set m = {a: 1, b: 2} :map[str i64]\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Let {
            value: Some(Expression::Dict { entries, .. }),
            ..
        } = &program.statements[0]
        else {
            panic!("expected dict literal");
        };

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn parses_multiline_match_expression_without_leading_block_open() {
        let source = "pub fn main: () {\nset x = 5 :i64\nset result = match x {\n    1 { 10 }\n    _ { 0 }\n} :i64\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Match { cases, .. }),
            ..
        } = &body[1]
        else {
            panic!("expected match expression");
        };

        assert_eq!(cases.len(), 1);
    }

    #[test]
    fn parses_inline_match_expression_case_bodies() {
        let source =
            "pub fn main: () {\nset x = 5 :i64\nset result = match x { 1 { 10 } _ { 0 } } :i64\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Let {
            value: Some(Expression::Match { cases, default, .. }),
            ..
        } = &body[1]
        else {
            panic!("expected match expression");
        };

        assert_eq!(cases.len(), 1);
        assert!(matches!(
            default.as_ref(),
            Expression::Literal(Literal::Int(0))
        ));
    }

    #[test]
    fn parses_match_statement_case_bodies_as_statements() {
        let source = "pub fn main: () {\nset x = 1 :i64\nmatch x {\n    1 { set y = 10 }\n}\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };
        let Statement::Match { cases, .. } = &body[1] else {
            panic!("expected match statement");
        };

        assert!(matches!(cases[0].1[0], Statement::Let { .. }));
    }

    #[test]
    fn desugars_pipeline_self_placeholder_into_direct_stage_expression() {
        let source = "pub fn main: () {\nuse range(5) => dasu({self})\n}\n";
        let program = parse(source).expect("parse should succeed");

        let Statement::Function { body, .. } = &program.statements[0] else {
            panic!("expected function");
        };

        assert!(matches!(
            body[0],
            Statement::Expression(Expression::Call { .. })
        ));
    }

    #[test]
    fn rejects_legacy_angle_block_syntax() {
        let source = "pub fn main: () >\nuse dasu(no)\n<\n";
        let program = parse(source);
        assert!(program.is_err(), "legacy angle blocks should be rejected");
    }
}