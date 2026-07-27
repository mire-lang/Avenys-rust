use crate::error::Result;
use crate::lexer::TokenType;
use crate::parser::ast::{DataType, Statement, TraitMethodSig, Visibility};

use super::Parser;

impl Parser {
    pub(super) fn extract_ascription_type(expr: &crate::parser::ast::Expression) -> Option<DataType> {
        use crate::parser::ast::Expression;
        match expr {
            Expression::Ascription { target, .. } => Some(target.clone()),
            _ => None,
        }
    }

    pub(super) fn parse_visibility(&mut self) -> Result<Visibility> {
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

    pub(super) fn parse_fn_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.expect(TokenType::Fn)?;
        let name_token = self.peek();
        let name_line = name_token.line;
        let name_column = name_token.column;
        let mut name = self.expect_ident()?;
        while self.check_double_colon() && Self::is_member_name_token(self.peek_n(2).ttype) {
            self.advance();
            self.advance();
            let member = self.expect_member_name()?;
            name = format!("{}::{}", name, member);
        }
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

        let attributes = std::mem::take(&mut self.pending_attributes);
        Ok(Statement::Function {
            name,
            attributes,
            type_params,
            type_param_bounds,
            params,
            body,
            return_type,
            visibility,
            is_method: self.method_context > 0,
            name_line,
            name_column,
        })
    }

    pub(super) fn parse_nominal_type_statement(
        &mut self,
        keyword: TokenType,
        visibility: Visibility,
    ) -> Result<Statement> {
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
                // Fields may be separated by commas (as in struct literals).
                if self.check(TokenType::Comma) {
                    self.advance();
                }
            } else if self.check(TokenType::Comma) {
                // Tolerate stray/leading commas between fields.
                self.advance();
            } else {
                // Defensive: never spin on an unexpected token. Advance to make
                // forward progress so a malformed body cannot hang the parser.
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect_block_close()?;
        self.pop_type_param_scope();
        self.declare(&name);
        Ok(Statement::Type {
            name,
            visibility,
            type_params,
            type_param_bounds,
            parent,
            fields,
        })
    }

    pub(super) fn parse_struct_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.parse_nominal_type_statement(TokenType::Struct, visibility)
    }

    pub(super) fn parse_type_statement(&mut self, visibility: Visibility) -> Result<Statement> {
        self.parse_nominal_type_statement(TokenType::Type, visibility)
    }

    pub(super) fn parse_skill_statement(&mut self, visibility: Visibility) -> Result<Statement> {
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
        Ok(Statement::Skill {
            name,
            visibility,
            methods,
        })
    }


}
