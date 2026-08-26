use super::*;
use crate::parser::ast::DataType;
use crate::parser::helpers::data_type_name;

type TypeParamBounds = Vec<(String, Vec<String>)>;

impl Parser {
    pub(super) fn parse_optional_type_params_with_bounds(
        &mut self,
    ) -> Result<(Vec<String>, TypeParamBounds)> {
        if !self.check(TokenType::Lbracket) {
            return Ok((Vec::new(), Vec::new()));
        }
        self.expect(TokenType::Lbracket)?;
        let mut params = Vec::new();
        let mut bounds = Vec::new();
        while !self.check(TokenType::Rbracket) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
            let pname = self.expect_ident()?;
            let mut pbounds = Vec::new();
            if self.check(TokenType::Colon) {
                self.advance();
                while !self.check(TokenType::Comma)
                    && !self.check(TokenType::Rbracket)
                    && !self.is_at_end()
                {
                    if self.check(TokenType::Plus) {
                        self.advance();
                        continue;
                    }
                    pbounds.push(self.expect_ident()?);
                }
            }
            if !pbounds.is_empty() {
                bounds.push((pname.clone(), pbounds));
            }
            params.push(pname);
        }
        self.expect(TokenType::Rbracket)?;
        Ok((params, bounds))
    }

    pub(super) fn parse_nominal_name_with_type_args(&mut self) -> Result<String> {
        let base = self.expect_ident()?;
        // Support qualified names: module::Type
        let name = if self.check_double_colon() {
            self.advance();
            self.advance();
            let tail = self.expect_ident()?;
            format!("{}.{}", base, tail)
        } else {
            base
        };
        if self.check(TokenType::Lbracket) {
            let args = self.parse_type_args()?;
            return Ok(format!(
                "{}[{}]",
                name,
                args.iter()
                    .map(data_type_name)
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        Ok(name)
    }

    pub(super) fn parse_type(&mut self) -> Result<DataType> {
        if self.check(TokenType::Amp) {
            self.advance();
            let is_mut = if self.check(TokenType::Mut) {
                self.advance();
                true
            } else {
                false
            };
            let inner = Box::new(self.parse_type()?);
            return Ok(if is_mut {
                DataType::RefMut { inner }
            } else {
                DataType::Ref { inner }
            });
        }

        if self.check(TokenType::NoneLit) {
            self.advance();
            return Ok(DataType::None);
        }

        if self.check(TokenType::Star) {
            self.advance();
            if self.check(TokenType::Mut) {
                self.advance();
            }
            if self.check(TokenType::Ident) {
                let _ = self.expect_ident()?;
            }
            return Ok(DataType::Unknown);
        }

        if self.check(TokenType::Ident) {
            let ident = self.expect_ident()?;
            return match ident.as_str() {
                "i8" => Ok(DataType::I8),
                "i16" => Ok(DataType::I16),
                "i32" => Ok(DataType::I32),
                "i64" => Ok(DataType::I64),
                "u8" => Ok(DataType::U8),
                "u16" => Ok(DataType::U16),
                "u32" => Ok(DataType::U32),
                "u64" => Ok(DataType::U64),
                "i128" => Ok(DataType::I128),
                "u128" => Ok(DataType::U128),
                "f32" => Ok(DataType::F32),
                "f64" => Ok(DataType::F64),
                "str" => Ok(DataType::Str),
                "bool" => Ok(DataType::Bool),
                "char" => Ok(DataType::Char),
                "mu" => Ok(DataType::None),
                "arr" => {
                    self.expect(TokenType::Lbracket)?;
                    let element_type = Box::new(self.parse_type()?);
                    let size_token = self.peek();
                    let size_text = self.expect_int_literal()?;
                    let size = size_text.parse().map_err(|_| {
                        self.error_at(
                            size_token.line,
                            size_token.column,
                            &format!("Invalid array size literal '{}'", size_text),
                        )
                    })?;
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Array { element_type, size })
                }
                "vec" => {
                    self.expect(TokenType::Lbracket)?;
                    let element_type = Box::new(self.parse_type()?);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Vector {
                        element_type,
                        dynamic: true,
                    })
                }
                "map" => {
                    if self.check(TokenType::Bang) {
                        self.advance();
                    };
                    self.expect(TokenType::Lbracket)?;
                    let key_type = Box::new(self.parse_type()?);
                    let value_type = Box::new(self.parse_type()?);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Map {
                        key_type,
                        value_type,
                    })
                }
                "result" => {
                    self.expect(TokenType::Lbracket)?;
                    let ok_type = Box::new(self.parse_type()?);
                    let err_type = if !self.check(TokenType::Rbracket) {
                        if self.check(TokenType::Comma) {
                            self.advance();
                        }
                        Box::new(self.parse_type()?)
                    } else {
                        Box::new(DataType::Str)
                    };
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Result {
                        ok: ok_type,
                        err: err_type,
                    })
                }
                "maybe" => {
                    self.expect(TokenType::Lbracket)?;
                    let inner_type = Box::new(self.parse_type()?);
                    self.expect(TokenType::Rbracket)?;
                    Ok(DataType::Maybe {
                        inner: inner_type,
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
                    } else if self.nominal_type_names.contains(other) {
                        if self.check(TokenType::Lbracket) {
                            let args = self.parse_type_args()?;
                            Ok(DataType::StructNamed(format!(
                                "{}[{}]",
                                other,
                                args.iter()
                                    .map(data_type_name)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )))
                        } else {
                            Ok(DataType::StructNamed(other.to_string()))
                        }
                    } else if self.enum_names.contains(other) {
                        if self.check(TokenType::Lbracket) {
                            let args = self.parse_type_args()?;
                            Ok(DataType::EnumNamed(format!(
                                "{}[{}]",
                                other,
                                args.iter()
                                    .map(data_type_name)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )))
                        } else {
                            Ok(DataType::EnumNamed(other.to_string()))
                        }
                    } else if self.is_type_param(other) {
                        Ok(DataType::Generic(other.to_string()))
                    } else {
                        Ok(DataType::parse_type(other))
                    }
                }
            };
        }

        Err(self.error("Expected type"))
    }

    pub(super) fn parse_type_args(&mut self) -> Result<Vec<DataType>> {
        self.expect(TokenType::Lbracket)?;
        let mut args = Vec::new();
        while !self.check(TokenType::Rbracket) && !self.is_at_end() {
            if self.check(TokenType::Comma) {
                self.advance();
                continue;
            }
            args.push(self.parse_type()?);
        }
        self.expect(TokenType::Rbracket)?;
        Ok(args)
    }

    pub(super) fn parse_type_name_string(&mut self) -> Result<String> {
        let start = self.pos;
        let _ = self.parse_type()?;
        let mut out = String::new();
        for token in &self.tokens[start..self.pos] {
            out.push_str(&self.token_surface(token.clone()));
        }
        Ok(out)
    }
}
