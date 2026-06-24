use super::*;

impl LlvmIrGen {
    pub(super) fn cast_scalar_for_store(
        &mut self,
        value: LlValue,
        data_type: &DataType,
    ) -> Result<(String, String)> {
        match data_type {
            DataType::Bool => {
                let bool_value = self.cast_to_i1(value)?;
                let widened = self.tmp();
                self.body
                    .push(format!("  {widened} = zext i1 {} to i8", bool_value.repr));
                Ok(("i8".to_string(), widened))
            }
            DataType::I8 | DataType::U8 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i8", scalar.repr));
                Ok(("i8".to_string(), narrowed))
            }
            DataType::I16 | DataType::U16 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i16", scalar.repr));
                Ok(("i16".to_string(), narrowed))
            }
            DataType::I32 | DataType::U32 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i32", scalar.repr));
                Ok(("i32".to_string(), narrowed))
            }
            _ => {
                let scalar = self.cast_to_i64(value)?;
                Ok(("i64".to_string(), scalar.repr))
            }
        }
    }

    pub(super) fn cast_to_i64(&mut self, value: LlValue) -> Result<LlValue> {
        match value.ty {
            LlType::I64 => Ok(value),
            LlType::I8 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = zext i8 {} to i64", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::I1 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = zext i1 {} to i64", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::F64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = fptosi double {} to i64", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Ptr => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = ptrtoint ptr {} to i64", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                message: "Cannot cast struct to i64".to_string(),
            })),
        }
    }

    pub(super) fn cast_to_i1(&mut self, value: LlValue) -> Result<LlValue> {
        match value.ty {
            LlType::I1 => Ok(value),
            LlType::I8 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = icmp ne i8 {}, 0", value.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::I64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = icmp ne i64 {}, 0", value.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::F64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = fcmp one double {}, 0.0", value.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Ptr => Err(MireError::new(ErrorKind::Runtime {
                message: "Cannot convert pointer to boolean".to_string(),
            })),
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                message: "Cannot cast struct to i1".to_string(),
            })),
        }
    }

    pub(super) fn compile_binary(
        &mut self,
        op: &str,
        lhs: LlValue,
        rhs: LlValue,
        data_type: &DataType,
    ) -> Result<LlValue> {
        let left_repr = lhs.repr.clone();
        let right_repr = rhs.repr.clone();
        let left_is_ptr = lhs.ty == LlType::Ptr;
        let right_is_ptr = rhs.ty == LlType::Ptr;
        let result = self.tmp();

        if left_is_ptr && right_is_ptr && op == "+" {
            self.body.push(format!(
                "  {result} = call ptr @rt_string_concat(ptr {left_repr}, ptr {right_repr})"
            ));
            if self.owned_temps.remove(&lhs.repr) {
                self.body
                    .push(format!("  call void @rt_managed_free(ptr {})", lhs.repr));
            }
            if self.owned_temps.remove(&rhs.repr) {
                self.body
                    .push(format!("  call void @rt_managed_free(ptr {})", rhs.repr));
            }
            self.owned_temps.insert(result.clone());
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: result,
                owned: true,
            });
        }

        if (left_is_ptr || right_is_ptr) && matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=") {
            let left = if left_is_ptr {
                lhs
            } else {
                self.ensure_ptr(lhs)
            };
            let right = if right_is_ptr {
                rhs
            } else {
                self.ensure_ptr(rhs)
            };
            let cmp_value = self.tmp();
            self.body.push(format!(
                "  {cmp_value} = call i32 @strcmp(ptr {}, ptr {})",
                left.repr, right.repr
            ));
            let pred = match op {
                "==" => "eq",
                "!=" => "ne",
                "<" => "slt",
                ">" => "sgt",
                "<=" => "sle",
                ">=" => "sge",
                _ => unreachable!(),
            };
            self.body
                .push(format!("  {result} = icmp {pred} i32 {cmp_value}, 0"));
            return Ok(LlValue {
                ty: LlType::I1,
                repr: result,
                owned: false,
            });
        }

        let should_use_float = lhs.ty == LlType::F64
            || rhs.ty == LlType::F64
            || matches!(data_type, DataType::F64 | DataType::F32);
        if should_use_float {
            let left_f64 = self.cast_to_f64(lhs.clone())?;
            let right_f64 = self.cast_to_f64(rhs.clone())?;
            match op {
                "+" => {
                    self.body.push(format!(
                        "  {result} = fadd double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    });
                }
                "-" => {
                    self.body.push(format!(
                        "  {result} = fsub double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    });
                }
                "*" => {
                    self.body.push(format!(
                        "  {result} = fmul double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    });
                }
                "/" => {
                    self.emit_nonzero_check_f64(&right_f64.repr, "division by zero");
                    self.body.push(format!(
                        "  {result} = fdiv double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    });
                }
                "%" => {
                    self.emit_nonzero_check_f64(&right_f64.repr, "division by zero");
                    self.body.push(format!(
                        "  {result} = frem double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    });
                }
                "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                    let cmp = match op {
                        "==" => "oeq",
                        "!=" => "one",
                        "<" => "olt",
                        ">" => "ogt",
                        "<=" => "ole",
                        ">=" => "oge",
                        _ => unreachable!(),
                    };
                    self.body.push(format!(
                        "  {result} = fcmp {cmp} double {}, {}",
                        left_f64.repr, right_f64.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::I1,
                        repr: result,
                        owned: false,
                    });
                }
                _ => {}
            }
        }

        match op {
            "+" => {
                self.body
                    .push(format!("  {result} = add i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "-" => {
                self.body
                    .push(format!("  {result} = sub i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "*" => {
                self.body
                    .push(format!("  {result} = mul i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "/" => {
                self.emit_nonzero_check(&right_repr, "division by zero");
                self.body
                    .push(format!("  {result} = sdiv i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "%" => {
                self.emit_nonzero_check(&right_repr, "division by zero");
                self.body
                    .push(format!("  {result} = srem i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                let cmp = match op {
                    "==" => "eq",
                    "!=" => "ne",
                    "<" => "slt",
                    ">" => "sgt",
                    "<=" => "sle",
                    ">=" => "sge",
                    _ => "eq",
                };
                self.body.push(format!(
                    "  {result} = icmp {cmp} i64 {left_repr}, {right_repr}"
                ));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            "&&" => {
                self.body
                    .push(format!("  {result} = and i1 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            "||" => {
                self.body
                    .push(format!("  {result} = or i1 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            "^" => {
                if lhs.ty == LlType::I1 && rhs.ty == LlType::I1 {
                    self.body
                        .push(format!("  {result} = xor i1 {left_repr}, {right_repr}"));
                    Ok(LlValue {
                        ty: LlType::I1,
                        repr: result,
                        owned: false,
                    })
                } else {
                    let left_i64 = self.cast_to_i64(lhs)?;
                    let right_i64 = self.cast_to_i64(rhs)?;
                    self.body.push(format!(
                        "  {result} = xor i64 {}, {}",
                        left_i64.repr, right_i64.repr
                    ));
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: result,
                        owned: false,
                    })
                }
            }
            "&" => {
                let left_i64 = self.cast_to_i64(lhs)?;
                let right_i64 = self.cast_to_i64(rhs)?;
                self.body.push(format!(
                    "  {result} = and i64 {}, {}",
                    left_i64.repr, right_i64.repr
                ));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "|" => {
                let left_i64 = self.cast_to_i64(lhs)?;
                let right_i64 = self.cast_to_i64(rhs)?;
                self.body.push(format!(
                    "  {result} = or i64 {}, {}",
                    left_i64.repr, right_i64.repr
                ));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "<<" => {
                let left_i64 = self.cast_to_i64(lhs)?;
                let right_i64 = self.cast_to_i64(rhs)?;
                self.body.push(format!(
                    "  {result} = shl i64 {}, {}",
                    left_i64.repr, right_i64.repr
                ));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            ">>" => {
                let left_i64 = self.cast_to_i64(lhs)?;
                let right_i64 = self.cast_to_i64(rhs)?;
                self.body.push(format!(
                    "  {result} = lshr i64 {}, {}",
                    left_i64.repr, right_i64.repr
                ));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Unknown operator: {}", op),
            })),
        }
    }

    pub(super) fn compile_logical_short_circuit(
        &mut self,
        op: &str,
        left: &Expression,
        right: &Expression,
        _data_type: &DataType,
    ) -> Result<LlValue> {
        let end_label = self.label("logical_end");
        let result_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {result_ptr} = alloca i1"));

        let left_val = self.compile_expr(left)?;
        let left_cond = self.cast_to_i1(left_val)?;

        if op == "&&" {
            let skip_label = self.label("and_skip_rhs");
            let rhs_label = self.label("and_rhs");
            self.body.push(format!(
                "  br i1 {}, label %{rhs_label}, label %{skip_label}",
                left_cond.repr
            ));
            self.body.push(format!("{skip_label}:"));
            self.body.push(format!("  store i1 0, ptr {result_ptr}"));
            self.body.push(format!("  br label %{end_label}"));
            self.body.push(format!("{rhs_label}:"));
            let right_val = self.compile_expr(right)?;
            let right_cond = self.cast_to_i1(right_val)?;
            self.body
                .push(format!("  store i1 {}, ptr {result_ptr}", right_cond.repr));
            self.body.push(format!("  br label %{end_label}"));
        } else {
            let skip_label = self.label("or_skip_rhs");
            let rhs_label = self.label("or_rhs");
            self.body.push(format!(
                "  br i1 {}, label %{skip_label}, label %{rhs_label}",
                left_cond.repr
            ));
            self.body.push(format!("{skip_label}:"));
            self.body.push(format!("  store i1 1, ptr {result_ptr}"));
            self.body.push(format!("  br label %{end_label}"));
            self.body.push(format!("{rhs_label}:"));
            let right_val = self.compile_expr(right)?;
            let right_cond = self.cast_to_i1(right_val)?;
            self.body
                .push(format!("  store i1 {}, ptr {result_ptr}", right_cond.repr));
            self.body.push(format!("  br label %{end_label}"));
        }

        self.body.push(format!("{end_label}:"));
        let loaded = self.tmp();
        self.body
            .push(format!("  {loaded} = load i1, ptr {result_ptr}"));
        Ok(LlValue {
            ty: LlType::I1,
            repr: loaded,
            owned: false,
        })
    }

    pub(super) fn compile_unary(&mut self, op: &str, value: LlValue) -> Result<LlValue> {
        let result = self.tmp();
        match op {
            "-" => {
                if value.ty == LlType::F64 {
                    let float_value = self.cast_to_f64(value)?;
                    self.body.push(format!(
                        "  {result} = fsub double 0.0, {}",
                        float_value.repr
                    ));
                    Ok(LlValue {
                        ty: LlType::F64,
                        repr: result,
                        owned: false,
                    })
                } else {
                    self.body
                        .push(format!("  {result} = sub i64 0, {}", value.repr));
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: result,
                        owned: false,
                    })
                }
            }
            "!" => {
                let bool_val = self.cast_to_i1(value)?;
                self.body
                    .push(format!("  {result} = xor i1 {}, 1", bool_val.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Unknown unary operator: {}", op),
            })),
        }
    }

    pub(super) fn cast_to_type(&mut self, value: LlValue, ty: LlType) -> Result<LlValue> {
        match ty {
            LlType::I64 => self.cast_to_i64(value),
            LlType::I1 => self.cast_to_i1(value),
            LlType::F64 => self.cast_to_f64(value),
            LlType::I8 => self.cast_to_i64(value),
            LlType::Struct(_) if value.ty == ty => Ok(value),
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                message: "Struct type not supported here".to_string(),
            })),
            LlType::Ptr if value.ty == LlType::Ptr => Ok(value),
            LlType::Ptr if value.ty == LlType::I64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = inttoptr i64 {} to ptr", value.repr));
                Ok(LlValue {
                    ty: LlType::Ptr,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Ptr => Err(MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys cannot cast non-pointer value (ty={:?}) to string (function '{}', ret={:?})",
                    value.ty, self.current_function, self.current_return
                ),
            })),
        }
    }

    pub(super) fn store_casted(&mut self, ptr: &str, ty: LlType, value: LlValue) -> Result<()> {
        let value = match ty {
            LlType::I64 => self.cast_to_i64(value)?,
            LlType::I1 => self.cast_to_i1(value)?,
            LlType::F64 => self.cast_to_f64(value)?,
            LlType::I8 => self.cast_to_i64(value)?,
            LlType::Struct(_) if value.ty == ty => value,
            LlType::Struct(_) => {
                return Err(MireError::new(ErrorKind::Backend {
                    message: "Struct type not supported here".to_string(),
                }));
            }
            LlType::Ptr if value.ty == LlType::Ptr => value,
            LlType::Ptr if value.ty == LlType::I64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = inttoptr i64 {} to ptr", value.repr));
                LlValue {
                    ty: LlType::Ptr,
                    repr: tmp,
                    owned: false,
                }
            }
            LlType::Ptr => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Avenys cannot cast non-pointer value to string (function '{}')",
                        self.current_function
                    ),
                }));
            }
        };
        self.body.push(format!(
            "  store {} {}, ptr {}",
            self.ty(ty),
            value.repr,
            ptr
        ));
        Ok(())
    }

    pub(super) fn cast_to_f64(&mut self, value: LlValue) -> Result<LlValue> {
        match value.ty {
            LlType::F64 => Ok(value),
            LlType::I64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = sitofp i64 {} to double", value.repr));
                Ok(LlValue {
                    ty: LlType::F64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::I1 => {
                let as_i64 = self.cast_to_i64(value)?;
                self.cast_to_f64(as_i64)
            }
            LlType::I8 => {
                let as_i64 = self.cast_to_i64(value)?;
                self.cast_to_f64(as_i64)
            }
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                message: "Struct type not supported here".to_string(),
            })),
            LlType::Ptr => Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot cast pointer/struct to float".to_string(),
            })),
        }
    }

    pub(super) fn store_variable(
        &mut self,
        name: &str,
        ptr: &str,
        ty: LlType,
        data_type: DataType,
        value: LlValue,
    ) -> Result<()> {
        if data_type == DataType::Str && ty == LlType::Ptr {
            let old_owned = self
                .vars
                .get(name)
                .map(|var| var.owns_heap_string)
                .unwrap_or(false);

            if old_owned {
                let old_ptr = self.tmp();
                self.body.push(format!("  {old_ptr} = load ptr, ptr {ptr}"));
                self.body
                    .push(format!("  call void @rt_managed_free(ptr {old_ptr})"));
            }

            let owned_value = if value.owned {
                value
            } else {
                let copied = self.tmp();
                self.body.push(format!(
                    "  {copied} = call ptr @rt_string_copy(ptr {})",
                    value.repr
                ));
                LlValue {
                    ty: LlType::Ptr,
                    repr: copied,
                    owned: true,
                }
            };

            self.owned_temps.remove(&owned_value.repr);
            self.store_casted(ptr, ty.clone(), owned_value)?;
            if let Some(var) = self.vars.get_mut(name) {
                var.data_type = data_type;
                var.owns_heap_string = true;
            }
            return Ok(());
        }

        self.store_casted(ptr, ty.clone(), value)?;
        if let Some(var) = self.vars.get_mut(name) {
            var.data_type = data_type;
            var.owns_heap_string = false;
        }
        Ok(())
    }

    pub(super) fn try_compile_in_place_string_append(
        &mut self,
        target: &str,
        var: &VarInfo,
        value: &Expression,
    ) -> Result<bool> {
        if var.data_type != DataType::Str || var.ty != LlType::Ptr || !var.owns_heap_string {
            return Ok(false);
        }

        let Expression::BinaryOp {
            operator,
            left,
            right,
            ..
        } = value
        else {
            return Ok(false);
        };

        if operator != "+" {
            return Ok(false);
        }

        let Expression::Identifier(identifier) = left.as_ref() else {
            return Ok(false);
        };

        if identifier.name != target {
            return Ok(false);
        }

        let rhs = self.compile_expr(right)?;
        let current = self.tmp();
        let appended = self.tmp();
        self.body
            .push(format!("  {current} = load ptr, ptr {}", var.ptr));
        self.body.push(format!(
            "  {appended} = call ptr @rt_string_append_owned(ptr {current}, ptr {})",
            rhs.repr
        ));
        self.body
            .push(format!("  store ptr {appended}, ptr {}", var.ptr));
        if let Some(var) = self.vars.get_mut(target) {
            var.owns_heap_string = true;
        }
        Ok(true)
    }
}
