use super::*;

impl LlvmIrGen {
    pub(super) fn compile_pipeline_len(
        &mut self,
        input: &Expression,
        value: LlValue,
    ) -> Result<LlValue> {
        match self.expression_data_type(input) {
            DataType::Str => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = call i64 @strlen(ptr {})", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            DataType::List | DataType::Vector { .. } => self.compile_list_len_value(value),
            _ => match value.ty {
                LlType::Ptr => self.compile_list_len_value(value),
                LlType::I64 | LlType::I1 | LlType::F64 | LlType::I8 => Ok(LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                }),
                LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                    message: "Struct type not supported here".to_string(),
                })),
            },
        }
    }

    pub(super) fn compile_pipeline_closure(
        &mut self,
        input_val: LlValue,
        params: &[(String, DataType)],
        body: &[Statement],
        return_type: &DataType,
    ) -> Result<LlValue> {
        if params.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Pipeline closure must have exactly 1 parameter".to_string(),
            }));
        }

        let param_name = &params[0].0;
        let param_type = params[0].1.clone();
        let result_element_type = if *return_type == DataType::Unknown {
            param_type.clone()
        } else {
            return_type.clone()
        };
        let elem_size = self.element_size(&result_element_type);

        let var_ptr = self.tmp();
        self.entry_allocas.push(format!("  {var_ptr} = alloca i64"));

        let list_result_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {list_result_ptr} = alloca ptr"));

        let index_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {index_ptr} = alloca i64"));

        let initial_list = self.tmp();
        self.body.push(format!(
            "  {initial_list} = call ptr @rt_list_create(i64 4, i64 {})",
            elem_size
        ));
        self.body
            .push(format!("  store ptr {initial_list}, ptr {list_result_ptr}"));
        self.body.push(format!("  store i64 0, ptr {index_ptr}"));

        let is_null = self.tmp();
        let loop_cond_label = self.label("pl_closure_cond");
        let loop_body_label = self.label("pl_closure_body");
        let end_label = self.label("pl_closure_end");
        self.body
            .push(format!("  {is_null} = icmp eq ptr {initial_list}, null"));
        self.body.push(format!(
            "  br i1 {is_null}, label %{end_label}, label %{loop_cond_label}"
        ));

        self.body.push(format!("{loop_cond_label}:"));
        let input_len = self.tmp();
        let index = self.tmp();
        let has_more = self.tmp();
        let current_list = self.tmp();

        self.body.push(format!(
            "  {current_list} = load ptr, ptr {list_result_ptr}"
        ));
        self.body
            .push(format!("  {input_len} = load i64, ptr {}", input_val.repr));
        self.body
            .push(format!("  {index} = load i64, ptr {index_ptr}"));
        self.body
            .push(format!("  {has_more} = icmp slt i64 {index}, {input_len}"));
        self.body.push(format!(
            "  br i1 {has_more}, label %{loop_body_label}, label %{end_label}"
        ));

        self.body.push(format!("{loop_body_label}:"));
        let data_ptr = self.tmp();
        let offset = self.tmp();
        let elem_ptr = self.tmp();

        self.body.push(format!(
            "  {data_ptr} = getelementptr i8, ptr {}, i64 8",
            input_val.repr
        ));
        self.body
            .push(format!("  {offset} = mul i64 {index}, {}", elem_size));
        self.body.push(format!(
            "  {elem_ptr} = getelementptr i8, ptr {data_ptr}, i64 {offset}"
        ));

        let elem_val = self.tmp();
        match param_type {
            DataType::Bool => {
                let raw = self.tmp();
                self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                self.body
                    .push(format!("  {elem_val} = trunc i8 {raw} to i1"));
            }
            DataType::I8 | DataType::U8 => {
                let raw = self.tmp();
                self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                self.body
                    .push(format!("  {elem_val} = zext i8 {raw} to i64"));
            }
            DataType::I16 | DataType::U16 => {
                let raw = self.tmp();
                self.body
                    .push(format!("  {raw} = load i16, ptr {elem_ptr}"));
                self.body
                    .push(format!("  {elem_val} = zext i16 {raw} to i64"));
            }
            DataType::I32 | DataType::U32 => {
                let raw = self.tmp();
                self.body
                    .push(format!("  {raw} = load i32, ptr {elem_ptr}"));
                self.body
                    .push(format!("  {elem_val} = zext i32 {raw} to i64"));
            }
            _ => {
                self.body
                    .push(format!("  {elem_val} = load i64, ptr {elem_ptr}"));
            }
        }

        let param_var_ptr = var_ptr.clone();
        let old_vars = self.vars.clone();
        self.vars.insert(
            param_name.clone(),
            VarInfo {
                ptr: param_var_ptr.clone(),
                ty: LlType::I64,
                data_type: param_type.clone(),
                owns_heap_string: false,
                struct_name: None,
            },
        );

        self.body
            .push(format!("  store i64 {}, ptr {}", elem_val, param_var_ptr));

        let result_val = self.compile_closure_body(body, return_type)?;
        self.vars = old_vars;

        let result_i64 = self.cast_to_i64(result_val)?;
        let result_i64_repr = result_i64.repr.clone();

        let result_list_new = self.tmp();
        if elem_size == 8 {
            self.body.push(format!(
                "  {result_list_new} = call ptr @rt_list_push_i64(ptr {current_list}, i64 {result_i64_repr})"
            ));
        } else {
            self.body.push(format!(
                "  {result_list_new} = call ptr @rt_list_push_scalar(ptr {current_list}, i64 {result_i64_repr}, i64 {})",
                elem_size
            ));
        }
        self.body.push(format!(
            "  store ptr {result_list_new}, ptr {list_result_ptr}"
        ));

        let next_index = self.tmp();
        self.body
            .push(format!("  {next_index} = add i64 {index}, 1"));
        self.body
            .push(format!("  store i64 {next_index}, ptr {index_ptr}"));
        self.body.push(format!("  br label %{loop_cond_label}"));

        self.body.push(format!("{end_label}:"));
        let final_list = self.tmp();
        self.body
            .push(format!("  {final_list} = load ptr, ptr {list_result_ptr}"));

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: final_list,
            owned: true,
        })
    }

    pub(super) fn compile_closure_body(
        &mut self,
        body: &[Statement],
        _expected_type: &DataType,
    ) -> Result<LlValue> {
        if body.is_empty() {
            return Ok(LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            });
        }

        for stmt in body.iter().take(body.len() - 1) {
            self.compile_statement(stmt)?;
        }

        if let Some(last) = body.last() {
            match last {
                Statement::Return(Some(expr)) => self.compile_expr(expr),
                Statement::Expression(expr) => self.compile_expr(expr),
                _ => {
                    self.compile_statement(last)?;
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: "0".to_string(),
                        owned: false,
                    })
                }
            }
        } else {
            Ok(LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            })
        }
    }

    pub(super) fn compile_bound_closure(
        &mut self,
        params: &[(String, DataType)],
        bound_values: &[LlValue],
        body: &[Statement],
        return_type: &DataType,
    ) -> Result<LlValue> {
        let old_vars = self.vars.clone();

        for ((name, data_type), value) in params.iter().zip(bound_values.iter()) {
            let ll_ty = self.map_type(data_type)?;
            let ptr = self.tmp();
            self.entry_allocas
                .push(format!("  {ptr} = alloca {}", self.ty(ll_ty.clone())));
            self.store_casted(&ptr, ll_ty.clone(), value.clone())?;
            self.vars.insert(
                name.clone(),
                VarInfo {
                    ptr,
                    ty: ll_ty,
                    data_type: data_type.clone(),
                    owns_heap_string: false,
                    struct_name: data_type.struct_name().map(ToOwned::to_owned),
                },
            );
        }

        let result = self.compile_closure_body(body, return_type)?;
        self.vars = old_vars;
        Ok(result)
    }

    pub(super) fn load_list_element_unchecked(
        &mut self,
        list_ptr: &str,
        index_repr: &str,
        element_type: &DataType,
    ) -> Result<LlValue> {
        let base_ptr = self.tmp();
        let offset = self.tmp();
        let elem_ptr = self.tmp();
        let elem_size = self.element_size(element_type);

        self.body.push(format!(
            "  {base_ptr} = getelementptr i8, ptr {list_ptr}, i64 8"
        ));
        self.body
            .push(format!("  {offset} = mul i64 {index_repr}, {elem_size}"));
        self.body.push(format!(
            "  {elem_ptr} = getelementptr i8, ptr {base_ptr}, i64 {offset}"
        ));

        let elem_ty = self.map_type(element_type)?;
        if elem_ty == LlType::Ptr {
            let val = self.tmp();
            self.body
                .push(format!("  {val} = load ptr, ptr {elem_ptr}"));
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: val,
                owned: false,
            });
        }

        if matches!(element_type, DataType::Bool) {
            let raw = self.tmp();
            let val = self.tmp();
            self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
            self.body.push(format!("  {val} = icmp ne i8 {raw}, 0"));
            return Ok(LlValue {
                ty: LlType::I1,
                repr: val,
                owned: false,
            });
        }

        let raw_ty = self.scalar_storage_ir_type(element_type);
        let raw = self.tmp();
        self.body
            .push(format!("  {raw} = load {raw_ty}, ptr {elem_ptr}"));
        let val = match raw_ty {
            "i8" => {
                let widened = self.tmp();
                let ext = if matches!(element_type, DataType::U8) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i8 {raw} to i64"));
                widened
            }
            "i16" => {
                let widened = self.tmp();
                let ext = if matches!(element_type, DataType::U16) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i16 {raw} to i64"));
                widened
            }
            "i32" => {
                let widened = self.tmp();
                let ext = if matches!(element_type, DataType::U32) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i32 {raw} to i64"));
                widened
            }
            _ => raw,
        };

        Ok(LlValue {
            ty: LlType::I64,
            repr: val,
            owned: false,
        })
    }

    pub(super) fn load_slot_value(&mut self, ptr: &str, data_type: &DataType) -> Result<LlValue> {
        let ll_ty = self.map_type(data_type)?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = load {}, ptr {ptr}",
            self.ty(ll_ty.clone())
        ));
        Ok(LlValue {
            ty: ll_ty,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn push_list_value(
        &mut self,
        list: LlValue,
        value: LlValue,
        data_type: &DataType,
    ) -> Result<LlValue> {
        let result = self.tmp();
        let ll_ty = self.map_type(data_type)?;
        let elem_size = self.element_size(data_type);

        if ll_ty == LlType::I64 && elem_size == 8 {
            let casted = self.cast_to_i64(value)?;
            self.body.push(format!(
                "  {result} = call ptr @rt_list_push_i64(ptr {}, i64 {})",
                list.repr, casted.repr
            ));
        } else {
            let casted = self.cast_to_i64(value)?;
            self.body.push(format!(
                "  {result} = call ptr @rt_list_push_scalar(ptr {}, i64 {}, i64 {})",
                list.repr, casted.repr, elem_size
            ));
        }

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_field_assignment(
        &mut self,
        target: &str,
        value: &Expression,
    ) -> Result<()> {
        let (field_ptr, field_ty, field_data_type) =
            self.resolve_struct_field_ptr_from_target(target)?;
        let compiled = self.compile_expr(value)?;

        if field_data_type == DataType::Str && field_ty == LlType::Ptr {
            let owned_value = if compiled.owned {
                compiled
            } else {
                let copied = self.tmp();
                self.body.push(format!(
                    "  {copied} = call ptr @rt_string_copy(ptr {})",
                    compiled.repr
                ));
                LlValue {
                    ty: LlType::Ptr,
                    repr: copied,
                    owned: true,
                }
            };
            self.store_casted(&field_ptr, field_ty, owned_value)?;
            return Ok(());
        }

        self.store_casted(&field_ptr, field_ty, compiled)
    }

    pub(super) fn compile_reference_expr(&mut self, expr: &Expression) -> Result<LlValue> {
        let (ptr, _) = self.resolve_lvalue_ptr(expr)?;
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: ptr,
            owned: false,
        })
    }

    pub(super) fn compile_dereference_expr(
        &mut self,
        expr: &Expression,
        data_type: &DataType,
    ) -> Result<LlValue> {
        let ptr = self.compile_expr(expr)?;
        if ptr.ty != LlType::Ptr {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot dereference non-pointer value".to_string(),
            }));
        }
        self.load_from_ptr(&ptr.repr, data_type)
    }

    pub(super) fn compile_index_assignment(
        &mut self,
        target: &Expression,
        index: &Expression,
        value: &Expression,
    ) -> Result<()> {
        let target_data_type = self.expression_data_type(target);
        let element_data_type = match &target_data_type {
            DataType::Array { element_type, .. }
            | DataType::Slice { element_type }
            | DataType::Vector { element_type, .. } => *element_type.clone(),
            _ => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Indexed assignment is not supported for type {:?}",
                        target_data_type
                    ),
                }));
            }
        };
        let (elem_ptr, elem_ty) =
            self.resolve_index_ptr(target, index, &target_data_type, &element_data_type)?;
        let compiled = self.compile_expr(value)?;
        self.store_casted(&elem_ptr, elem_ty, compiled)
    }

    pub(super) fn resolve_index_ptr(
        &mut self,
        target: &Expression,
        index: &Expression,
        target_data_type: &DataType,
        element_data_type: &DataType,
    ) -> Result<(String, LlType)> {
        let target_val = self.compile_expr(target)?;
        if target_val.ty != LlType::Ptr {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot assign through non-pointer index target".to_string(),
            }));
        }
        let compiled_index = self.compile_expr(index)?;
        let index_val = self.cast_to_i64(compiled_index)?;
        let elem_size = self.element_size(element_data_type);
        let (base_ptr, do_bounds_check) = match target_data_type {
            DataType::Array { size, .. } => {
                let len_val = LlValue {
                    ty: LlType::I64,
                    repr: size.to_string(),
                    owned: false,
                };
                self.emit_bounds_check(index_val.clone(), len_val, "index out of bounds");
                let base = self.tmp();
                self.body.push(format!(
                    "  {base} = getelementptr inbounds i8, ptr {}, i64 8",
                    target_val.repr
                ));
                (base, false)
            }
            DataType::Vector { .. } | DataType::Slice { .. } => {
                let base = self.tmp();
                self.body.push(format!(
                    "  {base} = getelementptr inbounds i8, ptr {}, i64 8",
                    target_val.repr
                ));
                (base, true)
            }
            other => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!("Type {:?} does not support indexed assignment", other),
                }));
            }
        };
        if do_bounds_check {
            let len = self.compile_list_len_value(target_val)?;
            self.emit_bounds_check(index_val.clone(), len, "index out of bounds");
        }
        let offset = self.tmp();
        self.body.push(format!(
            "  {offset} = mul i64 {}, {}",
            index_val.repr, elem_size
        ));
        let elem_ptr = self.tmp();
        self.body.push(format!(
            "  {elem_ptr} = getelementptr inbounds i8, ptr {base_ptr}, i64 {offset}"
        ));
        Ok((elem_ptr, self.map_type(element_data_type)?))
    }

    pub(super) fn resolve_lvalue_ptr(&mut self, expr: &Expression) -> Result<(String, DataType)> {
        match expr {
            Expression::Identifier(Identifier { name, .. }) => {
                let var = self.vars.get(name).cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Avenys unknown identifier '{}'", name),
                    })
                })?;
                Ok((var.ptr, var.data_type))
            }
            Expression::MemberAccess { target, member, .. } => {
                let (field_ptr, _, field_data_type) =
                    self.resolve_struct_field_ptr(target, &[member.as_str()])?;
                Ok((field_ptr, field_data_type))
            }
            Expression::Index {
                target,
                index,
                data_type,
            } => {
                let target_data_type = self.expression_data_type(target);
                let element_data_type = if *data_type != DataType::Unknown {
                    data_type.clone()
                } else {
                    self.expression_data_type(expr)
                };
                let (elem_ptr, _) =
                    self.resolve_index_ptr(target, index, &target_data_type, &element_data_type)?;
                Ok((elem_ptr, element_data_type))
            }
            other => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys cannot take a reference to expression {:?}", other),
            })),
        }
    }

    pub(super) fn load_from_ptr(&mut self, ptr: &str, data_type: &DataType) -> Result<LlValue> {
        let ll_ty = self.map_type(data_type)?;
        if ll_ty == LlType::Ptr {
            let value = self.tmp();
            self.body.push(format!("  {value} = load ptr, ptr {ptr}"));
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: value,
                owned: false,
            });
        }
        if ll_ty == LlType::I1 {
            let value = self.tmp();
            self.body.push(format!("  {value} = load i1, ptr {ptr}"));
            return Ok(LlValue {
                ty: LlType::I1,
                repr: value,
                owned: false,
            });
        }

        let raw_ty = self.scalar_storage_ir_type(data_type);
        let raw = self.tmp();
        self.body
            .push(format!("  {raw} = load {raw_ty}, ptr {ptr}"));
        let value = match raw_ty {
            "i8" => {
                let widened = self.tmp();
                let ext = if matches!(data_type, DataType::U8) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i8 {raw} to i64"));
                widened
            }
            "i16" => {
                let widened = self.tmp();
                let ext = if matches!(data_type, DataType::U16) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i16 {raw} to i64"));
                widened
            }
            "i32" => {
                let widened = self.tmp();
                let ext = if matches!(data_type, DataType::U32) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {widened} = {ext} i32 {raw} to i64"));
                widened
            }
            _ => raw,
        };

        Ok(LlValue {
            ty: LlType::I64,
            repr: value,
            owned: false,
        })
    }

    pub(super) fn resolve_struct_field_ptr_from_target(
        &mut self,
        target: &str,
    ) -> Result<(String, LlType, DataType)> {
        let mut segments = target.split('.');
        let owner = segments.next().ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Invalid field assignment target '{}'", target),
            })
        })?;
        let fields: Vec<_> = segments.collect();
        if fields.is_empty() {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!("Field assignment target '{}' has no field path", target),
            }));
        }

        let owner_expr = if owner == "self" {
            Expression::Identifier(Identifier {
                name: "self".to_string(),
                data_type: self
                    .vars
                    .get("self")
                    .map(|var| var.data_type.clone())
                    .unwrap_or(DataType::Unknown),
                line: 0,
                column: 0,
            })
        } else {
            let var = self.vars.get(owner).ok_or_else(|| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Avenys does not know variable '{}'", owner),
                })
            })?;
            Expression::Identifier(Identifier {
                name: owner.to_string(),
                data_type: var.data_type.clone(),
                line: 0,
                column: 0,
            })
        };

        self.resolve_struct_field_ptr(&owner_expr, &fields)
    }

    pub(super) fn resolve_struct_field_ptr(
        &mut self,
        target: &Expression,
        fields: &[&str],
    ) -> Result<(String, LlType, DataType)> {
        let target_val = self.compile_expr(target)?;
        let mut struct_name = self.struct_name_from_expr(target).ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys cannot resolve struct field path '{}'",
                    fields.join(".")
                ),
            })
        })?;
        let mut current_ptr = target_val.repr;

        for (index, member) in fields.iter().enumerate() {
            let struct_lookup = normalize_nominal_name(&struct_name);
            let struct_info = self
                .user_structs
                .get(&struct_lookup)
                .cloned()
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Unknown struct '{}'", struct_name),
                    })
                })?;
            let field_index = struct_info
                .field_indices
                .get(*member)
                .copied()
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Struct '{}' has no field '{}'", struct_name, member),
                    })
                })?;
            let struct_ty = self.render_struct_ty(&struct_info.fields);
            let field_ptr = self.tmp();
            self.body.push(format!(
                "  {field_ptr} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                struct_ty, current_ptr, field_index
            ));

            let field_ty = struct_info
                .fields
                .get(field_index)
                .cloned()
                .unwrap_or(LlType::I64);
            let field_data_type = struct_info
                .field_data_types
                .get(field_index)
                .cloned()
                .unwrap_or(DataType::Unknown);

            if index + 1 == fields.len() {
                return Ok((field_ptr, field_ty, field_data_type));
            }

            let next_struct_name = field_data_type
                .struct_name()
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Field '{}.{}' is not a nested struct",
                            struct_name, member
                        ),
                    })
                })?;
            let loaded = self.tmp();
            self.body
                .push(format!("  {loaded} = load ptr, ptr {field_ptr}"));
            current_ptr = loaded;
            struct_name = next_struct_name;
        }

        Err(MireError::new(ErrorKind::Runtime {
            message: format!("Invalid field assignment target '{}'", fields.join(".")),
        }))
    }

    pub(super) fn compile_member_access(
        &mut self,
        target: &Expression,
        member: &str,
    ) -> Result<LlValue> {
        let target_val = self.compile_expr(target)?;
        let struct_name = self.struct_name_from_expr(target).ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys cannot resolve struct member '{}' without concrete struct metadata",
                    member
                ),
            })
        })?;
        let struct_lookup = normalize_nominal_name(&struct_name);
        let struct_info = self
            .user_structs
            .get(&struct_lookup)
            .cloned()
            .ok_or_else(|| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Unknown struct '{}'", struct_name),
                })
            })?;
        let field_index = struct_info
            .field_indices
            .get(member)
            .copied()
            .ok_or_else(|| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Struct '{}' has no field '{}'", struct_name, member),
                })
            })?;
        let struct_ty = self.render_struct_ty(&struct_info.fields);
        let field_ptr = self.tmp();
        self.body.push(format!(
            "  {field_ptr} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
            struct_ty, target_val.repr, field_index
        ));
        let field_ty = struct_info
            .fields
            .get(field_index)
            .cloned()
            .unwrap_or(LlType::I64);

        match field_ty {
            LlType::I64 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load i64, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: value,
                    owned: false,
                })
            }
            LlType::I8 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load i8, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::I8,
                    repr: value,
                    owned: false,
                })
            }
            LlType::I1 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load i1, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: value,
                    owned: false,
                })
            }
            LlType::F64 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load double, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::F64,
                    repr: value,
                    owned: false,
                })
            }
            LlType::Ptr => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load ptr, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::Ptr,
                    repr: value,
                    owned: false,
                })
            }
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                message: "Struct type not supported here".to_string(),
            })),
        }
    }

    pub(super) fn compile_enum_variant_path(
        &mut self,
        enum_name: &str,
        variant_name: &str,
    ) -> Result<LlValue> {
        let (enum_ty, tag) = {
            let (enum_info, variant) = self.lookup_enum_variant(enum_name, variant_name)?;
            (enum_info.llvm_type.clone(), variant.tag)
        };
        let ptr = self.tmp();
        self.body.push(format!("  {ptr} = alloca {enum_ty}"));
        let tag_ptr = self.tmp();
        self.body.push(format!(
            "  {tag_ptr} = getelementptr inbounds {}, ptr {ptr}, i32 0, i32 0",
            enum_ty
        ));
        self.body
            .push(format!("  store i32 {}, ptr {tag_ptr}", tag));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: ptr,
            owned: false,
        })
    }

    pub(super) fn compile_enum_variant(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        payloads: &[Expression],
    ) -> Result<LlValue> {
        let (enum_ty, tag) = {
            let (enum_info, variant) = self.lookup_enum_variant(enum_name, variant_name)?;
            (enum_info.llvm_type.clone(), variant.tag)
        };
        let ptr = self.tmp();
        self.body.push(format!("  {ptr} = alloca {enum_ty}"));
        let tag_ptr = self.tmp();
        self.body.push(format!(
            "  {tag_ptr} = getelementptr inbounds {}, ptr {ptr}, i32 0, i32 0",
            enum_ty
        ));
        self.body
            .push(format!("  store i32 {}, ptr {tag_ptr}", tag));
        for (index, payload_expr) in payloads.iter().enumerate() {
            let payload_val = self.compile_expr(payload_expr)?;
            let payload_i64 = self.cast_to_i64(payload_val)?;
            let payload_ptr = self.tmp();
            self.body.push(format!(
                "  {payload_ptr} = getelementptr inbounds {}, ptr {ptr}, i32 0, i32 1, i32 {}",
                enum_ty, index
            ));
            self.body.push(format!(
                "  store i64 {}, ptr {payload_ptr}",
                payload_i64.repr
            ));
        }
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: ptr,
            owned: false,
        })
    }

    pub(super) fn compile_index(
        &mut self,
        target: LlValue,
        index: LlValue,
        target_data_type: &DataType,
        result_data_type: &DataType,
    ) -> Result<LlValue> {
        let mut target = target;
        if target.ty == LlType::I64 {
            let tmp = self.tmp();
            self.body
                .push(format!("  {tmp} = inttoptr i64 {} to ptr", target.repr));
            target = LlValue {
                ty: LlType::Ptr,
                repr: tmp,
                owned: false,
            };
        } else if target.ty != LlType::Ptr {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys cannot index non-pointer type (function '{}')",
                    self.current_function
                ),
            }));
        }

        match target_data_type {
            DataType::Unknown
            | DataType::List
            | DataType::Vector { .. }
            | DataType::Slice { .. }
            | DataType::Tuple => {
                let index = self.cast_to_i64(index)?;
                let len = self.compile_list_len_value(target.clone())?;
                self.emit_bounds_check(index.clone(), len, "index out of bounds");
                let elem_size = self.element_size(result_data_type);
                let base = self.tmp();
                self.body.push(format!(
                    "  {base} = getelementptr inbounds i8, ptr {}, i64 8",
                    target.repr
                ));
                let offset = self.tmp();
                self.body.push(format!(
                    "  {offset} = mul i64 {}, {}",
                    index.repr, elem_size
                ));
                let elem_ptr = self.tmp();
                self.body.push(format!(
                    "  {elem_ptr} = getelementptr inbounds i8, ptr {base}, i64 {offset}"
                ));
                let elem_ty = self.map_type(result_data_type)?;
                if elem_ty == LlType::Ptr {
                    let val = self.tmp();
                    self.body
                        .push(format!("  {val} = load ptr, ptr {elem_ptr}"));
                    Ok(LlValue {
                        ty: LlType::Ptr,
                        repr: val,
                        owned: false,
                    })
                } else if elem_ty == LlType::I1 {
                    let raw = self.tmp();
                    let val = self.tmp();
                    self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                    self.body.push(format!("  {val} = icmp ne i8 {raw}, 0"));
                    Ok(LlValue {
                        ty: LlType::I1,
                        repr: val,
                        owned: false,
                    })
                } else {
                    let raw_ty = self.scalar_storage_ir_type(result_data_type);
                    let raw = self.tmp();
                    self.body
                        .push(format!("  {raw} = load {raw_ty}, ptr {elem_ptr}"));
                    let val = match raw_ty {
                        "i8" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U8) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i8 {raw} to i64"));
                            widened
                        }
                        "i16" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U16) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i16 {raw} to i64"));
                            widened
                        }
                        "i32" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U32) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i32 {raw} to i64"));
                            widened
                        }
                        _ => raw,
                    };
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: val,
                        owned: false,
                    })
                }
            }
            DataType::Array { element_type, size } => {
                let index = self.cast_to_i64(index)?;
                let elem_size = self.element_size(element_type);
                let size_val = LlValue {
                    ty: LlType::I64,
                    repr: size.to_string(),
                    owned: false,
                };
                self.emit_bounds_check(index.clone(), size_val, "index out of bounds");
                let base = self.tmp();
                self.body.push(format!(
                    "  {base} = getelementptr inbounds i8, ptr {}, i64 8",
                    target.repr
                ));
                let offset_val = self.tmp();
                self.body.push(format!(
                    "  {offset_val} = mul i64 {}, {}",
                    index.repr, elem_size
                ));
                let elem_ptr = self.tmp();
                self.body.push(format!(
                    "  {elem_ptr} = getelementptr inbounds i8, ptr {base}, i64 {offset_val}"
                ));
                let elem_ty = self.map_type(element_type)?;
                if elem_ty == LlType::Ptr {
                    let val = self.tmp();
                    self.body
                        .push(format!("  {val} = load ptr, ptr {elem_ptr}"));
                    Ok(LlValue {
                        ty: LlType::Ptr,
                        repr: val,
                        owned: false,
                    })
                } else if elem_ty == LlType::I1 {
                    let raw = self.tmp();
                    let val = self.tmp();
                    self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                    self.body.push(format!("  {val} = icmp ne i8 {raw}, 0"));
                    Ok(LlValue {
                        ty: LlType::I1,
                        repr: val,
                        owned: false,
                    })
                } else {
                    let raw_ty = self.scalar_storage_ir_type(element_type);
                    let raw = self.tmp();
                    self.body
                        .push(format!("  {raw} = load {raw_ty}, ptr {elem_ptr}"));
                    let val = match raw_ty {
                        "i8" => {
                            let widened = self.tmp();
                            self.body
                                .push(format!("  {widened} = zext i8 {raw} to i64"));
                            widened
                        }
                        "i16" => {
                            let widened = self.tmp();
                            self.body
                                .push(format!("  {widened} = zext i16 {raw} to i64"));
                            widened
                        }
                        "i32" => {
                            let widened = self.tmp();
                            self.body
                                .push(format!("  {widened} = zext i32 {raw} to i64"));
                            widened
                        }
                        _ => raw,
                    };
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: val,
                        owned: false,
                    })
                }
            }
            DataType::Str => {
                let index = self.cast_to_i64(index)?;
                let len = self.tmp();
                self.body
                    .push(format!("  {len} = call i64 @strlen(ptr {})", target.repr));
                self.emit_bounds_check(
                    index.clone(),
                    LlValue {
                        ty: LlType::I64,
                        repr: len,
                        owned: false,
                    },
                    "index out of bounds",
                );
                let elem_ptr = self.tmp();
                self.body.push(format!(
                    "  {elem_ptr} = getelementptr inbounds i8, ptr {}, i64 {}",
                    target.repr, index.repr
                ));
                let byte = self.tmp();
                self.body
                    .push(format!("  {byte} = load i8, ptr {elem_ptr}"));
                let widened = self.tmp();
                self.body
                    .push(format!("  {widened} = zext i8 {byte} to i64"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: widened,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys cannot index type {:?}", target_data_type),
            })),
        }
    }

    pub(super) fn compile_type_matches(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys __type_matches expects 2 arguments".to_string(),
            }));
        }
        let expr_type = self.expression_data_type(&args[0]);
        let type_name = match &args[1] {
            Expression::Literal(Literal::Str(s)) => s.clone(),
            other => {
                let compiled = self.compile_expr(other)?;
                return Ok(LlValue {
                    ty: LlType::I1,
                    repr: compiled.repr,
                    owned: false,
                });
            }
        };
        let expected_type = match type_name.as_str() {
            "i64" => DataType::I64,
            "str" => DataType::Str,
            "bool" => DataType::Bool,
            "f64" => DataType::F64,
            "vec[anything]" | "vec" => DataType::Vector {
                element_type: Box::new(DataType::Anything),
                dynamic: true,
            },
            "map[anything anything]" | "map" => DataType::Map {
                key_type: Box::new(DataType::Anything),
                value_type: Box::new(DataType::Anything),
            },
            _ => {
                return Ok(LlValue {
                    ty: LlType::I1,
                    repr: "false".to_string(),
                    owned: false,
                });
            }
        };
        let expr_llvm_ty = self.map_type(&expr_type).unwrap_or(LlType::I64);
        let expected_llvm_ty = self.map_type(&expected_type).unwrap_or(LlType::I64);
        let result = if expr_llvm_ty == expected_llvm_ty
            || expr_type == DataType::Unknown
            || expr_type == DataType::Anything
        {
            "true".to_string()
        } else {
            "false".to_string()
        };
        Ok(LlValue {
            ty: LlType::I1,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_contains(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys contains(...) expects 2 arguments".to_string(),
            }));
        }
        let haystack_type = self.expression_data_type(&args[0]);
        if haystack_type != DataType::Str {
            return Err(MireError::new(ErrorKind::Backend {
                message: "Avenys contains(...) is currently lowered for strings only".to_string(),
            }));
        }
        let haystack = self.compile_expr(&args[0])?;
        let needle = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_strings_contains(ptr {}, ptr {})",
            haystack.repr, needle.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_struct_constructor(
        &mut self,
        type_name: &str,
        args: &[Expression],
    ) -> Result<LlValue> {
        let struct_info = self.user_structs.get(type_name).cloned().ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Unknown struct '{}'", type_name),
            })
        })?;

        let ptr = self.tmp();
        self.body.push(format!(
            "  {ptr} = call ptr @malloc(i64 {})",
            struct_info.fields.len() * 8
        ));

        let struct_ty = self.render_struct_ty(&struct_info.fields);

        for arg in args {
            if let Expression::NamedArg { name, value, .. } = arg
                && let Some(field_index) = struct_info.field_indices.get(name)
            {
                let field_value = self.compile_expr(value)?;
                let field_ptr = self.tmp();
                self.body.push(format!(
                    "  {field_ptr} = getelementptr inbounds {}, ptr {ptr}, i32 0, i32 {}",
                    struct_ty, field_index
                ));

                let field_type = struct_info
                    .fields
                    .get(*field_index)
                    .cloned()
                    .unwrap_or(LlType::I64);
                match field_type {
                    LlType::I64 => {
                        let casted = self.cast_to_i64(field_value)?;
                        self.body
                            .push(format!("  store i64 {}, ptr {field_ptr}", casted.repr));
                    }
                    LlType::I8 => {
                        let casted = self.cast_to_i64(field_value)?;
                        self.body
                            .push(format!("  store i64 {}, ptr {field_ptr}", casted.repr));
                    }
                    LlType::I1 => {
                        let casted = self.cast_to_i1(field_value)?;
                        self.body
                            .push(format!("  store i1 {}, ptr {field_ptr}", casted.repr));
                    }
                    LlType::F64 => {
                        self.body.push(format!(
                            "  store double {}, ptr {field_ptr}",
                            field_value.repr
                        ));
                    }
                    LlType::Ptr => {
                        self.body
                            .push(format!("  store ptr {}, ptr {field_ptr}", field_value.repr));
                    }
                    LlType::Struct(_) => {
                        return Err(MireError::new(ErrorKind::Backend {
                            message: "Struct type not supported here".to_string(),
                        }));
                    }
                }
            }
        }

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: ptr,
            owned: true,
        })
    }

    pub(super) fn concat_values(&mut self, lhs: LlValue, rhs: LlValue) -> LlValue {
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @rt_string_concat(ptr {}, ptr {})",
            lhs.repr, rhs.repr
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
        LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        }
    }
}
