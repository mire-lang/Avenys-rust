use crate::error::Result;
use crate::lexer::TokenType;
use crate::parser::ast::{DataType, Expression};

use super::Parser;
use super::helpers::string_expr;

impl Parser {
    pub(super) fn parse_or(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_xor(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_and(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_equality(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_bitwise_or(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_bitwise_and(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_comparison(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_additive(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_shift(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_multiplicative(&mut self) -> Result<Expression> {
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

    pub(super) fn parse_unary(&mut self) -> Result<Expression> {
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

}
