use crate::error::Result;
use crate::lexer::{Token, TokenType};
use crate::parser::ast::{
    DataType, EnumVariantDef, Expression, Literal, Statement, TraitMethodSig, Visibility,
};

use super::Parser;

impl Parser {
    pub(super) fn parse_statement(&mut self) -> Result<Statement> {
        if self.is_legacy_add_statement() {
            let token = self.peek();
            return Err(crate::error::MireError::deprecated_syntax(
                token.line,
                token.column,
                "Legacy `add` imports are no longer supported; use `load` instead"
                    .to_string(),
            ));
        }

        match self.peek().ttype {
            TokenType::Load => self.parse_load_statement(),
            TokenType::Module => self.parse_module_statement(),
            TokenType::Set => self.parse_set_statement(),
            TokenType::Use => {
                // use always produces an expression statement (side-effect call).
                // use dasu("hello"), use foo(), use pipeline => ...
                Ok(Statement::Expression(self.parse_use_expr()?))
            }
            TokenType::Pub | TokenType::Priv => {
                let visibility = self.parse_visibility()?;
                match self.peek().ttype {
                    TokenType::Fn => self.parse_fn_statement(visibility),
                    TokenType::Type => self.parse_type_statement(visibility),
                    TokenType::Skill => self.parse_skill_statement(visibility),
                    TokenType::Struct => self.parse_struct_statement(visibility),
                    TokenType::Enum => self.parse_enum_statement(visibility),
                    TokenType::Extern => self.parse_extern_statement_with_vis(visibility),
                    _ => {
                        Err(self
                            .error("Expected fn, type, skill, struct, enum, or extern after visibility"))
                    }
                }
            }
            TokenType::Fn => self.parse_fn_statement(Visibility::Private),
            TokenType::Type => self.parse_type_statement(Visibility::Private),
            TokenType::Skill => self.parse_skill_statement(Visibility::Private),
            TokenType::Struct => self.parse_struct_statement(Visibility::Private),
            TokenType::Impl => self.parse_impl_statement(),
            TokenType::Enum => self.parse_enum_statement(Visibility::Private),
            TokenType::Extern => self.parse_extern_statement(),
            TokenType::Unsafe => self.parse_unsafe_statement(),
            TokenType::Asm => self.parse_asm_statement(),
            TokenType::If => self.parse_if_statement(),
            TokenType::While => self.parse_while_statement(),
            TokenType::For => self.parse_for_statement(),
            TokenType::Find => self.parse_find_statement(),
            TokenType::Do => self.parse_do_while_statement(),
            TokenType::Match => self.parse_match_statement(),
            TokenType::NewKw => self.parse_new_statement(),
            TokenType::OwnKw => self.parse_own_statement(),
            TokenType::MoveKw => self.parse_move_statement(),
            TokenType::DropKw => self.parse_drop_statement(),
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

    fn is_legacy_add_statement(&self) -> bool {
        let current = self.peek();
        if current.ttype != TokenType::Ident || current.value.as_deref() != Some("add") {
            return false;
        }

        matches!(
            self.peek_n(1).ttype,
            TokenType::Ident | TokenType::Dot | TokenType::StrLit
        )
    }

    fn parse_set_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Set)?;

        let var_token = self.peek();
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

        let mut value = self.parse_expression()?;
        let declared_type = if self.check(TokenType::Colon) {
            self.advance();
            let dt = self.parse_type()?;
            match dt.clone() {
                DataType::Vector {
                    element_type,
                    dynamic,
                } => {
                    crate::parser::apply_vector_type_to_list(&mut value, element_type, dynamic);
                }
                DataType::Map {
                    key_type,
                    value_type,
                } => {
                    crate::parser::apply_map_type_to_dict(&mut value, key_type, value_type);
                }
                _ => {}
            }
            Some(dt)
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
            let left = target.as_expression();
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

        if matches!(
            target,
            crate::parser::ast::AssignmentTarget::Field(_)
                | crate::parser::ast::AssignmentTarget::Index { .. }
        ) {
            return Ok(Statement::Assignment {
                target,
                value,
                is_mutable: true,
            });
        }

        let crate::parser::ast::AssignmentTarget::Variable(target_name) = &target else {
            unreachable!("non-variable assignment target handled above");
        };
        let already_declared = self.is_declared(target_name);
        if declared_type.is_none() && !is_constant && already_declared {
            return Ok(Statement::Assignment {
                target,
                value,
                is_mutable: true,
            });
        }

        let data_type = declared_type.unwrap_or(DataType::Unknown);
        self.declare(target_name);
        Ok(Statement::Let {
            name: target_name.clone(),
            data_type,
            value: Some(value),
            is_constant,
            is_mutable,
            is_static: false,
            visibility: Visibility::Private,
            name_line: var_token.line,
            name_column: var_token.column,
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
        let (type_params, type_param_bounds) = self.parse_optional_type_params_with_bounds()?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::Lparen)?;
        self.push_type_param_scope(type_params.clone());
        let params = self.parse_param_list()?;
        self.expect(TokenType::Rparen)?;

        let return_type = if self.check(TokenType::Colon) {
            self.advance();
            self.parse_type()?
        } else {
            DataType::None
        };

        self.expect_block_open()?;
        self.push_scope();
        for (param_name, _) in &params {
            self.declare(param_name);
        }
        self.function_body_depth += 1;
        let body = self.parse_block()?;
        self.function_body_depth -= 1;
        self.pop_scope();
        self.expect_block_close()?;
        self.pop_type_param_scope();
        self.declare(&name);

        Ok(Statement::Function {
            name,
            type_params,
            type_param_bounds,
            params,
            body,
            return_type,
            visibility,
            is_method: self.method_context > 0,
        })
    }

    fn parse_nominal_type_statement(
        &mut self,
        keyword: TokenType,
        visibility: Visibility,
    ) -> Result<Statement> {
        let _ = visibility;
        self.expect(keyword)?;
        let name = self.expect_ident()?;
        let (type_params, type_param_bounds) = self.parse_optional_type_params_with_bounds()?;
        self.push_type_param_scope(type_params.clone());

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
            if self.peek().ttype == TokenType::Ident {
                let field_token = self.peek();
                let field_name = self.expect_ident()?;
                let field_type = if self.check(TokenType::Colon) {
                    self.advance();
                    self.parse_type()?
                } else {
                    DataType::Unknown
                };
                let is_mutable = if self.check(TokenType::Mut) {
                    self.advance();
                    true
                } else {
                    false
                };
                fields.push(Statement::Let {
                    name: field_name,
                    data_type: field_type,
                    value: None,
                    is_constant: false,
                    is_mutable,
                    is_static: false,
                    visibility: Visibility::Private,
                    name_line: field_token.line,
                    name_column: field_token.column,
                });
            }
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.pop_type_param_scope();
        self.declare(&name);
        Ok(Statement::Type {
            name,
            type_params,
            type_param_bounds,
            parent,
            fields,
        })
    }

    fn parse_struct_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.parse_nominal_type_statement(TokenType::Struct, visibility)
    }

    fn parse_type_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.parse_nominal_type_statement(TokenType::Type, visibility)
    }

    fn parse_skill_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        let _ = visibility;
        self.expect(TokenType::Skill)?;
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
            let params = self.parse_param_list()?;
            self.expect(TokenType::Rparen)?;

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

    fn parse_impl_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Impl)?;
        let (type_params, type_param_bounds) = self.parse_optional_type_params_with_bounds()?;
        self.push_type_param_scope(type_params.clone());
        let first = self.parse_nominal_name_with_type_args()?;
        let (trait_name, type_name) = if self.check(TokenType::For) {
            self.advance();
            (Some(first), self.parse_nominal_name_with_type_args()?)
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
        self.pop_type_param_scope();

        Ok(Statement::Impl {
            trait_name,
            type_name,
            type_params,
            type_param_bounds,
            methods,
        })
    }

    fn parse_enum_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        let _ = visibility;
        self.expect(TokenType::Enum)?;
        let enum_name = self.expect_ident()?;
        let (type_params, type_param_bounds) = self.parse_optional_type_params_with_bounds()?;
        self.push_type_param_scope(type_params.clone());
        self.expect_block_open()?;
        let mut variants = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            let variant_name = self.expect_ident()?;
            let (payload_names, payload_types) = if self.check(TokenType::Lparen) {
                self.advance();
                let mut names = Vec::new();
                let mut types = Vec::new();
                while !self.check(TokenType::Rparen) && !self.is_at_end() {
                    if self.check(TokenType::Comma) {
                        self.advance();
                        continue;
                    }
                    let binding = self.expect_ident()?;
                    self.expect(TokenType::Colon)?;
                    names.push(binding);
                    types.push(self.parse_type()?);
                    if self.check(TokenType::Comma) {
                        self.advance();
                    }
                }
                self.expect(TokenType::Rparen)?;
                (names, types)
            } else {
                (Vec::new(), Vec::new())
            };
            variants.push(EnumVariantDef {
                enum_name: enum_name.clone(),
                name: variant_name,
                payload_names,
                data_types: payload_types,
            });
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.pop_type_param_scope();
        self.declare(&enum_name);
        Ok(Statement::Enum {
            name: enum_name,
            type_params,
            type_param_bounds,
            variants,
        })
    }

    fn parse_if_statement(&mut self) -> Result<Statement> {
        let if_token = self.peek();
        self.expect(TokenType::If)?;
        let condition = self.parse_expression_until_block_open()?;

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
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;

        Ok(Statement::For {
            variable: first,
            index: second,
            iterable,
            body,
        })
    }

    fn parse_find_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Find)?;
        let variable = self.expect_ident()?;
        self.expect(TokenType::In)?;
        let iterable = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        self.push_scope();
        self.declare(&variable);
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        Ok(Statement::Find {
            variable,
            iterable,
            body,
        })
    }

    fn parse_unsafe_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Unsafe)?;
        self.expect_block_open()?;
        self.push_scope();
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        Ok(Statement::Unsafe { body })
    }

    fn parse_extern_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Extern)?;
        match self.peek().ttype {
            TokenType::Lib => self.parse_extern_lib_statement(),
            TokenType::Fn => self.parse_extern_fn_statement(Visibility::Private),
            _ => Err(self.error("Expected `lib` or `fn` after `extern`")),
        }
    }

    fn parse_extern_statement_with_vis(&mut self, visibility: Visibility) -> Result<Statement> {
        self.expect(TokenType::Extern)?;
        match self.peek().ttype {
            TokenType::Fn => self.parse_extern_fn_statement(visibility),
            _ => Err(self.error("Expected `fn` after visibility on extern")),
        }
    }

    fn parse_extern_lib_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Lib)?;
        let name = self.expect_string_or_ident()?;
        let path = self.expect_string_or_ident()?;
        Ok(Statement::ExternLib { name, path })
    }

    fn parse_extern_fn_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.expect(TokenType::Fn)?;
        let name = self.expect_ident()?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::Lparen)?;
        let params = self.parse_extern_param_list()?;
        self.expect(TokenType::Rparen)?;
        let return_type = if self.check(TokenType::Colon) {
            self.advance();
            self.parse_ffi_type()?
        } else {
            DataType::None
        };
        self.expect(TokenType::Lib)?;
        let lib_name = self.expect_string_or_ident()?;
        Ok(Statement::ExternFunction {
            name,
            lib_name,
            params,
            return_type,
            visibility,
        })
    }

    fn parse_extern_param_list(&mut self) -> Result<Vec<(String, DataType)>> {
        let mut params = Vec::new();
        while !self.check(TokenType::Rparen) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
            let name = self.expect_ident()?;
            let data_type = if self.check(TokenType::Colon) {
                self.advance();
                self.parse_ffi_type()?
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

    fn parse_ffi_type(&mut self) -> Result<DataType> {
        if self.check(TokenType::Star) {
            self.advance();
            if self.check(TokenType::Const) {
                self.advance();
            }
            if self.check(TokenType::Mut) {
                self.advance();
            }
            if self.check(TokenType::Ident) {
                self.advance();
            }
            return Ok(DataType::I64);
        }
        self.parse_type()
    }

    fn parse_asm_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Asm)?;
        self.expect_block_open()?;
        let mut instructions = Vec::new();

        while !self.check_block_close() && !self.is_at_end() {
            self.skip_newlines();
            if self.check_block_close() {
                break;
            }
            let opcode = self.expect_ident()?;
            let mut operands = Vec::new();
            while !self.check(TokenType::Newline) && !self.check_block_close() && !self.is_at_end()
            {
                operands.push(self.advance());
            }
            if self.check(TokenType::Newline) {
                self.advance();
            }
            let operand_text = operands
                .iter()
                .map(Self::token_to_asm_fragment)
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            instructions.push((opcode, Expression::Literal(Literal::Str(operand_text))));
        }

        self.expect_block_close()?;
        Ok(Statement::Asm { instructions })
    }

    fn token_to_asm_fragment(token: &Token) -> String {
        if let Some(value) = &token.value {
            return value.clone();
        }
        match token.ttype {
            TokenType::Comma => ",".to_string(),
            TokenType::Colon => ":".to_string(),
            TokenType::Dot => ".".to_string(),
            TokenType::Lparen => "(".to_string(),
            TokenType::Rparen => ")".to_string(),
            TokenType::Lbracket => "[".to_string(),
            TokenType::Rbracket => "]".to_string(),
            TokenType::Plus => "+".to_string(),
            TokenType::Minus => "-".to_string(),
            TokenType::Star => "*".to_string(),
            TokenType::Slash => "/".to_string(),
            TokenType::Percent => "%".to_string(),
            _ => format!("{:?}", token.ttype),
        }
    }

    fn expect_string_or_ident(&mut self) -> Result<String> {
        match self.peek().ttype {
            TokenType::StrLit | TokenType::Ident => Ok(self.advance().value.unwrap_or_default()),
            _ => Err(self.error("Expected string literal or identifier")),
        }
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
            type_args: Vec::new(),
            data_type: DataType::None,
        }))
    }

    fn parse_return_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Return)?;
        if self.is_statement_terminator() {
            return Ok(Statement::Return(None));
        }
        let expr = self.parse_expression()?;
        Ok(Statement::Return(Some(expr)))
    }

    fn parse_module_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::Module)?;
        let name = self.expect_ident()?;
        Ok(Statement::Module { name })
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
}
