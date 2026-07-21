use super::MirLower;
use super::collections::lower_index_write;
use super::types::{extract_data_type, llvm_elem_type_str, llvm_type_byte_size};
use crate::compiler::location::statement_location;
use crate::compiler::mir::*;
use crate::parser::ast::{AssignmentTarget, DataType, Expression, Statement};

/// True when `emit_convert` can safely lower a value of `from` into `to`
/// (both are integer/float/char kinds). Avoids the `Sitofp` fallback that
/// would corrupt non-numeric values such as structs or pointers.
pub(crate) fn needs_convert(from: &DataType, to: &DataType) -> bool {
    use DataType::*;
    let numeric = |t: &DataType| matches!(
        t,
        I8 | I16 | I32 | I64 | I128 | U8 | U16 | U32 | U64 | U128 | Char | F32 | F64
    );
    from != to && *to != DataType::Unknown && numeric(from) && numeric(to)
}

impl MirLower {
    pub(crate) fn lower_statement(&mut self, stmt: &Statement) {
        let loc = statement_location(stmt);
        match stmt {
            Statement::Let {
                name,
                data_type,
                value,
                ..
            } => {
                if self.globals.contains_key(name) {
                    if self.func.blocks.is_empty() {
                        self.func.push_block("entry".to_string());
                    }
                    if let Some(val) = value {
                        let val_ty = extract_data_type(val);
                        let mut v = self.lower_expression(val);
                        if needs_convert(&val_ty, data_type) {
                            v = self.emit_convert(v, &val_ty, data_type, loc);
                        }
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            None,
                            MirOp::Store(MirValue::Global(name.clone()), v),
                            loc,
                        );
                    }
                    return;
                }
                if self.func.blocks.is_empty() {
                    self.func.push_block("entry".to_string());
                }
                let last = self.current_block;
                let ptr = self.new_temp();
                self.func.blocks[last].push(
                    Some(ptr),
                    MirOp::Alloca(MirType {
                        data_type: data_type.clone(),
                    }),
                    loc,
                );
                self.vars.insert(name.clone(), ptr);
                self.var_types.insert(name.clone(), data_type.clone());

                if let Some(val) = value {
                    if let DataType::Array { element_type, .. } = data_type
                        && let Expression::List { elements, .. } = val
                    {
                        let elem_llvm = llvm_elem_type_str(element_type);
                        for (i, elem) in elements.iter().enumerate() {
                            let elem_val = self.lower_expression(elem);
                            let last = self.current_block;
                            let gep = self.new_temp();
                            self.func.blocks[last].push(
                                Some(gep),
                                MirOp::Gep(
                                    MirValue::temp(ptr),
                                    vec![MirValue::Const(MirConst::Int(i as i64))],
                                    elem_llvm.clone(),
                                ),
                                loc,
                            );
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::temp(gep), elem_val),
                                loc,
                            );
                        }
                        return;
                    }
                    let val_ty = extract_data_type(val);
                    let mut v = self.lower_expression(val);
                    if needs_convert(&val_ty, data_type) {
                        v = self.emit_convert(v, &val_ty, data_type, loc);
                    }
                    let last = self.current_block;
                    self.func.blocks[last].push(None, MirOp::Store(MirValue::temp(ptr), v), loc);
                }
            }
            Statement::Assignment { target, value, .. } => {
                let val_ty = extract_data_type(value);
                let mut v = self.lower_expression(value);
                let last = self.current_block;
                match target {
                    AssignmentTarget::Variable(name) => {
                        if let Some(target_ty) = self.var_types.get(name).cloned() {
                            if needs_convert(&val_ty, &target_ty) {
                                v = self.emit_convert(v, &val_ty, &target_ty, loc);
                            }
                        }
                        if self.globals.contains_key(name) {
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::Global(name.clone()), v),
                                loc,
                            );
                        } else if let Some(&ptr) = self.vars.get(name) {
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::temp(ptr), v),
                                loc,
                            );
                        }
                    }
                    AssignmentTarget::Index { target, index } => {
                        let target_type = if let Expression::Identifier(id) = target.as_ref() {
                            self.var_types.get(&id.name).cloned().unwrap_or(DataType::Unknown)
                        } else {
                            extract_data_type(target)
                        };
                        let value_type = extract_data_type(value);
                        if lower_index_write(self, target, index, v.clone(), &value_type) {
                            return;
                        }

                        let target_val = self.lower_expression(target);
                        let index_val = self.lower_expression(index);
                        let last = self.current_block;

                        match &target_type {
                            DataType::Array { size, .. } => {
                                self.func.blocks[last].push(
                                    None,
                                    MirOp::Call(
                                        MirValue::Global("rt_check_bounds_i64".to_string()),
                                        vec![
                                            index_val.clone(),
                                            MirValue::Const(MirConst::Int(*size as i64)),
                                            MirValue::Const(MirConst::Int(loc.0 as i64)),
                                            MirValue::Const(MirConst::Int(loc.1 as i64)),
                                            MirValue::Const(MirConst::Str(self.filename.clone())),
                                        ],
                                        MirType {
                                            data_type: DataType::None,
                                        },
                                    ),
                                    loc,
                                );
                            }
                            DataType::Vector { .. } | DataType::List => {
                                let len_val = self.new_temp();
                                self.func.blocks[last].push(
                                    Some(len_val),
                                    MirOp::Call(
                                        MirValue::Global("rt_list_len".to_string()),
                                        vec![target_val.clone()],
                                        MirType {
                                            data_type: DataType::I64,
                                        },
                                    ),
                                    loc,
                                );
                                self.func.blocks[last].push(
                                    None,
                                    MirOp::Call(
                                        MirValue::Global("rt_check_bounds_i64".to_string()),
                                        vec![
                                            index_val.clone(),
                                            MirValue::temp(len_val),
                                            MirValue::Const(MirConst::Int(loc.0 as i64)),
                                            MirValue::Const(MirConst::Int(loc.1 as i64)),
                                            MirValue::Const(MirConst::Str(self.filename.clone())),
                                        ],
                                        MirType {
                                            data_type: DataType::None,
                                        },
                                    ),
                                    loc,
                                );
                            }
                            _ => {}
                        }

                        let gep = self.new_temp();
                        let elem_ty = self.get_target_elem_type(target);
                        let adjusted_index = if matches!(
                            target_type,
                            DataType::Vector { .. } | DataType::List
                        ) {
                            let elem_size = llvm_type_byte_size(&elem_ty);
                            let header_offset = 8 / elem_size;
                            let adj = self.new_temp();
                            self.func.blocks[last].push(
                                Some(adj),
                                MirOp::Add(
                                    index_val.clone(),
                                    MirValue::Const(MirConst::Int(header_offset)),
                                ),
                                loc,
                            );
                            MirValue::temp(adj)
                        } else {
                            index_val.clone()
                        };
                        self.func.blocks[last].push(
                            Some(gep),
                            MirOp::Gep(target_val, vec![adjusted_index], elem_ty),
                            loc,
                        );
                        self.func.blocks[last].push(
                            None,
                            MirOp::Store(MirValue::temp(gep), v),
                            loc,
                        );
                    }
                    AssignmentTarget::Field(path) => {
                        let parts: Vec<&str> = path.splitn(2, '.').collect();
                        if parts.len() == 2 {
                            let var_name = parts[0].to_string();
                            let field_name = parts[1].to_string();
                            let ptr = self.vars.get(&var_name).copied();
                            let var_type = self.var_types.get(&var_name).cloned();
                            if let (Some(ptr), Some(var_type)) = (ptr, var_type)
                                && let DataType::StructNamed(ref struct_name) = var_type
                            {
                                let norm_name = struct_name
                                    .split_once('[')
                                    .map(|(base, _)| base.to_string())
                                    .unwrap_or_else(|| struct_name.clone());
                                let field_idx =
                                    self.struct_types.get(&norm_name).and_then(|fields| {
                                        fields.iter().position(|(n, _)| n == &field_name)
                                    });
                                if let Some(field_idx) = field_idx {
                                    let data_ptr = self.new_temp();
                                    self.func.blocks[last].push(
                                        Some(data_ptr),
                                        MirOp::Load(
                                            MirValue::temp(ptr),
                                            MirType {
                                                data_type: var_type,
                                            },
                                        ),
                                        loc,
                                    );
                                    let gep = self.new_temp();
                                    self.func.blocks[last].push(
                                        Some(gep),
                                        MirOp::Gep(
                                            MirValue::temp(data_ptr),
                                            vec![
                                                MirValue::Const(MirConst::Int(0)),
                                                MirValue::Const(MirConst::Int(field_idx as i64)),
                                            ],
                                            norm_name,
                                        ),
                                        loc,
                                    );
                                    self.func.blocks[last].push(
                                        None,
                                        MirOp::Store(MirValue::temp(gep), v),
                                        loc,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Statement::Expression(expr) => {
                let _ = self.lower_expression(expr);
            }
            Statement::Return(val) => {
                let v = val.as_ref().map(|e| self.lower_expression(e));
                let last = self.current_block;
                self.func.blocks[last].terminator = MirTerminator::Ret(v);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expression(condition);
                let pre_if = self.current_block;

                let then_block = self.new_block("if_then");
                self.current_block = then_block;

                for stmt in then_branch {
                    self.lower_statement(stmt);
                }
                let then_last = self.current_block;

                let else_block = self.new_block("if_else");
                self.current_block = else_block;

                if let Some(else_body) = else_branch {
                    for stmt in else_body {
                        self.lower_statement(stmt);
                    }
                }
                let else_last = self.current_block;

                let end_block = self.new_block("if_end");
                if matches!(
                    self.func.blocks[then_last].terminator,
                    MirTerminator::Unreachable
                ) {
                    self.func.blocks[then_last].terminator = MirTerminator::Br(end_block);
                }
                if matches!(
                    self.func.blocks[else_last].terminator,
                    MirTerminator::Unreachable
                ) {
                    self.func.blocks[else_last].terminator = MirTerminator::Br(end_block);
                }
                self.func.blocks[pre_if].terminator =
                    MirTerminator::BrCond(cond, then_block, else_block);
                self.current_block = end_block;
            }
            Statement::While {
                condition, body, ..
            } => {
                let pre_while_block = self.current_block;

                let cond_block = self.new_block("while_cond");
                self.current_block = cond_block;

                let cond = self.lower_expression(condition);

                let body_block = self.new_block("while_body");
                self.current_block = body_block;

                for stmt in body {
                    self.lower_statement(stmt);
                }
                if matches!(
                    self.func.blocks[self.current_block].terminator,
                    MirTerminator::Unreachable
                ) {
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(cond_block);
                }

                let end_block = self.new_block("while_end");
                self.func.blocks[cond_block].terminator =
                    MirTerminator::BrCond(cond, body_block, end_block);
                self.func.blocks[pre_while_block].terminator = MirTerminator::Br(cond_block);
                self.current_block = end_block;
            }
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => {
                let idx_ptr = self.new_temp();

                self.func.blocks[self.current_block].push(
                    Some(idx_ptr),
                    MirOp::Alloca(MirType {
                        data_type: DataType::I64,
                    }),
                    loc,
                );

                let iter_val = self.lower_expression(iterable);

                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(MirValue::temp(idx_ptr), MirValue::Const(MirConst::Int(0))),
                    loc,
                );

                let pre_for_block = self.current_block;
                let cond_block = self.new_block("for_cond");

                let idx_loaded = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(idx_loaded),
                    MirOp::Load(
                        MirValue::temp(idx_ptr),
                        MirType {
                            data_type: DataType::I64,
                        },
                    ),
                    loc,
                );
                let len_val = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(len_val),
                    MirOp::Call(
                        MirValue::Global("rt_list_len".to_string()),
                        vec![iter_val.clone()],
                        MirType {
                            data_type: DataType::I64,
                        },
                    ),
                    loc,
                );
                let cond = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(cond),
                    MirOp::ICmp(
                        MirCmp::Lt,
                        MirValue::temp(idx_loaded),
                        MirValue::temp(len_val),
                    ),
                    loc,
                );

                let body_block = self.new_block("for_body");
                self.current_block = body_block;

                let elem_ptr = self.new_temp();
                let elem_idx_i64 = self.new_temp();
                let elem_offset_i64 = self.new_temp();
                let elem_idx = self.new_temp();
                self.func.blocks[body_block].push(
                    Some(elem_idx_i64),
                    MirOp::Load(
                        MirValue::temp(idx_ptr),
                        MirType {
                            data_type: DataType::I64,
                        },
                    ),
                    loc,
                );
                self.func.blocks[body_block].push(
                    Some(elem_offset_i64),
                    MirOp::Add(
                        MirValue::temp(elem_idx_i64),
                        MirValue::Const(MirConst::Int(1)),
                    ),
                    loc,
                );
                self.func.blocks[body_block].push(
                    Some(elem_idx),
                    MirOp::Trunc(
                        MirValue::temp(elem_offset_i64),
                        MirType {
                            data_type: DataType::I32,
                        },
                    ),
                    loc,
                );
                self.func.blocks[body_block].push(
                    Some(elem_ptr),
                    MirOp::Gep(
                        iter_val.clone(),
                        vec![MirValue::temp(elem_idx)],
                        "ptr".to_string(),
                    ),
                    loc,
                );
                let elem_val = self.new_temp();
                self.func.blocks[body_block].push(
                    Some(elem_val),
                    MirOp::Load(
                        MirValue::temp(elem_ptr),
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    ),
                    loc,
                );

                let var_ptr = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(var_ptr),
                    MirOp::Alloca(MirType {
                        data_type: DataType::Unknown,
                    }),
                    loc,
                );
                self.vars.insert(variable.clone(), var_ptr);
                self.var_types.insert(variable.clone(), DataType::Unknown);
                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(MirValue::temp(var_ptr), MirValue::temp(elem_val)),
                    loc,
                );

                if let Some(idx_name) = index {
                    self.vars.insert(idx_name.clone(), idx_ptr);
                    self.var_types.insert(idx_name.clone(), DataType::I64);
                }

                for stmt in body {
                    self.lower_statement(stmt);
                }

                let last_body = self.current_block;
                let body_terminated = !matches!(
                    self.func.blocks[last_body].terminator,
                    MirTerminator::Unreachable
                );
                if !body_terminated {
                    let old_idx = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(old_idx),
                        MirOp::Load(
                            MirValue::temp(idx_ptr),
                            MirType {
                                data_type: DataType::I64,
                            },
                        ),
                        loc,
                    );
                    let new_idx = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(new_idx),
                        MirOp::Add(MirValue::temp(old_idx), MirValue::Const(MirConst::Int(1))),
                        loc,
                    );
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(idx_ptr), MirValue::temp(new_idx)),
                        loc,
                    );

                    self.func.blocks[last_body].terminator = MirTerminator::Br(cond_block);
                }

                let end_block = self.new_block("for_end");
                self.func.blocks[cond_block].terminator =
                    MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);
                self.func.blocks[pre_for_block].terminator = MirTerminator::Br(cond_block);
                self.current_block = end_block;
            }
            Statement::Unsafe { body, .. } => {
                for stmt in body {
                    self.lower_statement(stmt);
                }
            }
            _ => {}
        }
    }
}
