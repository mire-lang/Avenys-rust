use crate::error::Result;
use crate::lexer::TokenType;
use crate::parser::ast::Statement;

use super::Parser;

impl Parser {
    pub(super) fn parse_if_statement(&mut self) -> Result<Statement> {
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

    pub(super) fn parse_if_statement_from_elif(&mut self) -> Result<Statement> {
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

    pub(super) fn parse_while_statement(&mut self) -> Result<Statement> {
        self.expect(TokenType::While)?;
        let condition = self.parse_expression_until_block_open()?;
        self.expect_block_open()?;
        self.push_scope();
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        Ok(Statement::While { condition, body })
    }

    pub(super) fn parse_for_statement(&mut self) -> Result<Statement> {
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

    pub(super) fn parse_find_statement(&mut self) -> Result<Statement> {
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

    pub(super) fn parse_unsafe_statement(&mut self) -> Result<Statement> {
        let token = self.peek();
        let line = token.line;
        let column = token.column;
        self.expect(TokenType::Unsafe)?;
        self.expect_block_open()?;
        self.push_scope();
        let body = self.parse_block()?;
        self.pop_scope();
        self.expect_block_close()?;
        Ok(Statement::Unsafe { line, column, body })
    }


}
