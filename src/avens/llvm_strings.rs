use super::*;

impl LlvmIrGen {
    pub(super) fn compile_concat(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() < 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys concat(...) expects at least 2 arguments".to_string(),
            }));
        }

        let mut iter = args.iter().filter(
            |arg| !matches!(arg, Expression::Literal(Literal::Str(value)) if value.is_empty()),
        );

        let Some(first) = iter.next() else {
            return Ok(self.string_value(""));
        };

        let mut acc = self.compile_expr(first)?;
        for arg in iter {
            let value = self.compile_expr(arg)?;
            acc = self.concat_values(acc, value);
        }
        Ok(acc)
    }

    pub(super) fn compile_replace(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys replace(...) expects 3 arguments".to_string(),
            }));
        }

        if let (
            Expression::Literal(Literal::Str(input)),
            Expression::Literal(Literal::Str(from)),
            Expression::Literal(Literal::Str(to)),
        ) = (&args[0], &args[1], &args[2])
        {
            return Ok(self.string_value(&input.replace(from, to)));
        }

        if let (_, Expression::Literal(Literal::Str(from)), Expression::Literal(Literal::Str(to))) =
            (&args[0], &args[1], &args[2])
            && (from.is_empty() || from == to)
        {
            return self.compile_expr(&args[0]);
        }

        let input = self.compile_expr(&args[0])?;
        let from = self.compile_expr(&args[1])?;
        let to = self.compile_expr(&args[2])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_replace(ptr {}, ptr {}, ptr {})",
            input.repr, from.repr, to.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_split(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.split(...) expects 2 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let delimiter = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_split_list(ptr {}, ptr {})",
            input.repr, delimiter.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_join(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.join(...) expects 2 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let delimiter = self.compile_expr(&args[1])?;
        let count = self.compile_list_len_value(input.clone())?;
        let data_ptr = self.tmp();
        self.body.push(format!(
            "  {data_ptr} = getelementptr inbounds i8, ptr {}, i64 8",
            input.repr
        ));
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_join(ptr {data_ptr}, i64 {}, ptr {})",
            count.repr, delimiter.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_trim(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.trim(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(input.trim()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_trim(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_to_upper(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys to_upper(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(&input.to_ascii_uppercase()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_string_to_upper(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_to_lower(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys to_lower(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(&input.to_ascii_lowercase()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_string_to_lower(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_to_string(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.to_string(...) expects 1 argument".to_string(),
            }));
        }
        let input_val = self.compile_expr(&args[0])?;
        let input = self.ensure_ptr(input_val);
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_dict_to_string(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_replace_first(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.replace_first(...) expects 3 arguments".to_string(),
            }));
        }

        if let (
            Expression::Literal(Literal::Str(input)),
            Expression::Literal(Literal::Str(from)),
            Expression::Literal(Literal::Str(to)),
        ) = (&args[0], &args[1], &args[2])
        {
            if let Some(pos) = input.find(from) {
                let mut result = input[..pos].to_string();
                result.push_str(to);
                result.push_str(&input[pos + from.len()..]);
                return Ok(self.string_value(&result));
            }
            return Ok(self.string_value(input));
        }

        let input = self.compile_expr(&args[0])?;
        let from = self.compile_expr(&args[1])?;
        let to = self.compile_expr(&args[2])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_replace_first(ptr {}, ptr {}, ptr {})",
            input.repr, from.repr, to.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_starts_with(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.starts_with(...) expects 2 arguments".to_string(),
            }));
        }

        if let (
            Expression::Literal(Literal::Str(input)),
            Expression::Literal(Literal::Str(prefix)),
        ) = (&args[0], &args[1])
        {
            let result = input.starts_with(prefix.as_str());
            return Ok(LlValue {
                ty: LlType::I64,
                repr: if result {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
                owned: false,
            });
        }

        let input = self.compile_expr(&args[0])?;
        let prefix = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_strings_starts_with(ptr {}, ptr {})",
            input.repr, prefix.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_ends_with(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.ends_with(...) expects 2 arguments".to_string(),
            }));
        }

        if let (
            Expression::Literal(Literal::Str(input)),
            Expression::Literal(Literal::Str(suffix)),
        ) = (&args[0], &args[1])
        {
            let result = input.ends_with(suffix.as_str());
            return Ok(LlValue {
                ty: LlType::I64,
                repr: if result {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
                owned: false,
            });
        }

        let input = self.compile_expr(&args[0])?;
        let suffix = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_strings_ends_with(ptr {}, ptr {})",
            input.repr, suffix.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_substr(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.substr(...) expects 3 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let start_expr = self.compile_expr(&args[1])?;
        let start = self.cast_to_i64(start_expr)?;
        let len_expr = self.compile_expr(&args[2])?;
        let len = self.cast_to_i64(len_expr)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_substr(ptr {}, i64 {}, i64 {})",
            input.repr, start.repr, len.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_pad_left(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() < 2 || args.len() > 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.pad_left(...) expects 2 or 3 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let width_expr = self.compile_expr(&args[1])?;
        let width = self.cast_to_i64(width_expr)?;
        let pad = if args.len() == 3 {
            self.compile_expr(&args[2])?
        } else {
            self.string_value(" ")
        };
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_pad_left(ptr {}, i64 {}, ptr {})",
            input.repr, width.repr, pad.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_pad_right(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() < 2 || args.len() > 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.pad_right(...) expects 2 or 3 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let width_expr = self.compile_expr(&args[1])?;
        let width = self.cast_to_i64(width_expr)?;
        let pad = if args.len() == 3 {
            self.compile_expr(&args[2])?
        } else {
            self.string_value(" ")
        };
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_strings_pad_right(ptr {}, i64 {}, ptr {})",
            input.repr, width.repr, pad.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn emit_print(&mut self, value: &LlValue) -> Result<()> {
        match value.ty {
            LlType::I64 => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_i64, i64 {})",
                    value.repr
                ));
            }
            LlType::I8 => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_i64, i64 {})",
                    value.repr
                ));
            }
            LlType::Struct(_) => return Err(MireError::new(ErrorKind::Backend {
                message: "Struct type not supported here".to_string(),
            })),
            LlType::Ptr => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                    value.repr
                ));
            }
            LlType::I1 => {
                let true_ptr = self.string_value("true");
                let false_ptr = self.string_value("false");
                let select = self.tmp();
                self.body.push(format!(
                    "  {select} = select i1 {}, ptr {}, ptr {}",
                    value.repr, true_ptr.repr, false_ptr.repr
                ));
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {select})"
                ));
            }
            LlType::F64 => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_f64, double {})",
                    value.repr
                ));
            }
        }
        self.body.push("  call i32 @fflush(ptr null)".to_string());
        Ok(())
    }

    pub(super) fn emit_dasu_expr(&mut self, expr: &Expression) -> Result<()> {
        let value = self.compile_expr(expr)?;
        match self.expression_data_type(expr) {
            DataType::Dict | DataType::Map { .. } => {
                let rendered = self.tmp();
                self.body.push(format!(
                    "  {rendered} = call ptr @rt_dict_to_string(ptr {})",
                    value.repr
                ));
                self.emit_print(&LlValue {
                    ty: LlType::Ptr,
                    repr: rendered.clone(),
                    owned: true,
                })?;
                self.body.push(format!(
                    "  call void @rt_managed_free(ptr {rendered})"
                ));
            }
            _ => self.emit_print(&value)?,
        }
        Ok(())
    }
}
