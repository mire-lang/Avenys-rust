use super::MirLower;
use super::collections::lower_index_read;
use super::types::{
    data_type_to_kind, extract_data_type, is_map_or_dict_type, is_trivial_deref,
    llvm_elem_type_str, llvm_type_byte_size,
};
use crate::compiler::location::{NO_POSITION, expression_location};
use crate::compiler::mir::*;
use crate::parser::ast::{DataType, Expression, Literal, Statement};

fn is_float_dt(t: &DataType) -> bool {
    matches!(t, DataType::F32 | DataType::F64)
}

impl MirLower {
    pub(crate) fn lower_call_args(&mut self, name: &str, args: &[Expression]) -> Vec<MirValue> {
        let needs_wrap = name == "dasu" || name == "print" || name == "str";
        args.iter()
            .map(|a| {
                let arg_type = extract_data_type(a);
                let lowered = self.lower_expression(a);
                if needs_wrap && is_map_or_dict_type(&arg_type) {
                    let str_result = self.new_temp();
                    let last = self.current_block;
                    let a_loc = expression_location(a).to_tuple();
                    self.func.blocks[last].push(
                        Some(str_result),
                        MirOp::Call(
                            MirValue::Global("rt_dict_to_string".to_string()),
                            vec![lowered],
                            MirType {
                                data_type: DataType::Unknown,
                            },
                        ),
                        a_loc,
                    );
                    MirValue::temp(str_result)
                } else {
                    lowered
                }
            })
            .collect()
    }

    pub(crate) fn lower_expression(&mut self, expr: &Expression) -> MirValue {
        let loc = expression_location(expr).to_tuple();
        match expr {
            Expression::Ascription {
                expr: inner,
                target,
                ..
            } => {
                let val = self.lower_expression(inner);
                let inner_ty = extract_data_type(inner);
                if inner_ty != DataType::Unknown && inner_ty != *target {
                    return self.emit_convert(val, &inner_ty, target, loc);
                }
                val
            }
            Expression::Literal(lit) => {
                let val = self.lower_literal(lit);
                let natural = crate::types::unify::literal_type(lit);
                let expr_ty = extract_data_type(expr);
                if expr_ty != DataType::Unknown && expr_ty != natural {
                    return self.emit_convert(val, &natural, &expr_ty, loc);
                }
                val
            }
            Expression::Identifier(id) => {
                if let Some(&ptr) = self.vars.get(&id.name) {
                    let ty = self
                        .var_types
                        .get(&id.name)
                        .cloned()
                        .unwrap_or(DataType::Unknown);
                    if matches!(&ty, DataType::Array { .. }) {
                        return MirValue::temp(ptr);
                    }
                    let loaded = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(loaded),
                        MirOp::Load(
                            MirValue::temp(ptr),
                            MirType {
                                data_type: ty.clone(),
                            },
                        ),
                        loc,
                    );
                    let loaded_val = MirValue::temp(loaded);
                    if id.data_type != DataType::Unknown && id.data_type != ty {
                        return self.emit_convert(loaded_val, &ty, &id.data_type, loc);
                    }
                    loaded_val
                } else if let Some(gty) = self.globals.get(&id.name) {
                    let gty = gty.clone();
                    let loaded = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(loaded),
                        MirOp::Load(
                            MirValue::Global(id.name.clone()),
                            MirType {
                                data_type: gty.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(loaded)
                } else {
                    MirValue::Global(id.name.clone())
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
                ..
            } => {
                let l = self.lower_expression(left);
                let r = self.lower_expression(right);
                let result = self.new_temp();
                let mir_op = match operator.as_str() {
                    "+" => MirOp::Add(l, r),
                    "-" => MirOp::Sub(l, r),
                    "*" => MirOp::Mul(l, r),
                    "/" => MirOp::SDiv(l, r),
                    "%" => MirOp::SRem(l, r),
                    "==" => MirOp::ICmp(MirCmp::Eq, l, r),
                    "!=" => MirOp::ICmp(MirCmp::Ne, l, r),
                    "<" => MirOp::ICmp(MirCmp::Lt, l, r),
                    "<=" => MirOp::ICmp(MirCmp::Le, l, r),
                    ">" => MirOp::ICmp(MirCmp::Gt, l, r),
                    ">=" => MirOp::ICmp(MirCmp::Ge, l, r),
                    "&&" => MirOp::And(l, r),
                    "||" => MirOp::Or(l, r),
                    "&" => MirOp::BitAnd(l, r),
                    "|" => MirOp::BitOr(l, r),
                    "^" => MirOp::Xor(l, r),
                    "<<" => MirOp::Shl(l, r),
                    ">>" => MirOp::Shr(l, r),
                    _ => MirOp::Add(l, r),
                };
                let last = self.current_block;
                self.func.blocks[last].push(Some(result), mir_op, loc);
                let mut out = MirValue::temp(result);
                // Arithmetic ops are computed in a promoted width (i64 / float);
                // narrow the result back to the binary op's declared type.
                if matches!(operator.as_str(), "+" | "-" | "*" | "/" | "%") {
                    let left_ty = extract_data_type(left);
                    let right_ty = extract_data_type(right);
                    let natural = if is_float_dt(&left_ty) || is_float_dt(&right_ty) {
                        if left_ty == DataType::F64 || right_ty == DataType::F64 {
                            DataType::F64
                        } else {
                            DataType::F32
                        }
                    } else {
                        DataType::I64
                    };
                    if *data_type != DataType::Unknown && super::stmt::needs_convert(&natural, data_type) {
                        out = self.emit_convert(out, &natural, data_type, loc);
                    }
                }
                out
            }
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } if name == "__if_expr" && args.len() == 3 => {
                let cond = self.lower_expression(&args[0]);
                let then_expr = Self::extract_closure_expr(&args[1]);
                let else_expr = Self::extract_closure_expr(&args[2]);
                let then_val = self.lower_expression(then_expr);
                let else_val = self.lower_expression(else_expr);
                let result = self.new_temp();
                let ret_ty = MirType {
                    data_type: data_type.clone(),
                };

                let pre_ifexpr = self.current_block;
                self.func.blocks[pre_ifexpr].push(Some(result), MirOp::Alloca(ret_ty.clone()), loc);

                let then_block = self.new_block("ifexpr_then");
                let else_block = self.new_block("ifexpr_else");
                let end_block = self.new_block("ifexpr_end");

                self.func.blocks[pre_ifexpr].terminator =
                    MirTerminator::BrCond(cond, then_block, else_block);

                self.func.blocks[then_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result), then_val),
                    loc,
                );
                self.func.blocks[then_block].terminator = MirTerminator::Br(end_block);

                self.func.blocks[else_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result), else_val),
                    loc,
                );
                self.func.blocks[else_block].terminator = MirTerminator::Br(end_block);

                let loaded = self.new_temp();
                self.func.blocks[end_block].push(
                    Some(loaded),
                    MirOp::Load(MirValue::temp(result), ret_ty),
                    loc,
                );
                self.current_block = end_block;
                MirValue::temp(loaded)
            }
            Expression::Call { name, args, .. } if name == "__type_matches" => {
                MirValue::Const(MirConst::Bool(true))
            }
            Expression::Call { name, args, .. } if name == "range" => {
                let mir_args: Vec<MirValue> =
                    args.iter().map(|a| self.lower_expression(a)).collect();
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::Global("rt_math_range_i64".to_string()),
                        mir_args,
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call { name, args, .. } if name == "len" && !args.is_empty() => {
                let arg_val = self.lower_expression(&args[0]);
                let arg_type = extract_data_type(&args[0]);
                let rt_name = match arg_type {
                    DataType::Str | DataType::Ref { .. } | DataType::RefMut { .. } => {
                        "rt_strings_len"
                    }
                    DataType::Vector { .. } | DataType::List => "rt_list_len",
                    DataType::Map { .. } | DataType::Dict => "rt_dicts_len",
                    _ => "rt_list_len",
                };
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::Global(rt_name.to_string()),
                        vec![arg_val],
                        MirType {
                            data_type: DataType::I64,
                        },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call { name, args, .. } if name == "contains" && args.len() == 2 => {
                let haystack_val = self.lower_expression(&args[0]);
                let needle_val = self.lower_expression(&args[1]);
                let arg_type = extract_data_type(&args[0]);
                let rt_name = match &arg_type {
                    DataType::Str | DataType::Ref { .. } | DataType::RefMut { .. } => {
                        "rt_strings_contains"
                    }
                    DataType::Vector { element_type, .. }
                        if matches!(
                            &**element_type,
                            DataType::I64 | DataType::I128 | DataType::U128 | DataType::Unknown | DataType::Anything
                        ) =>
                    {
                        "rt_lists_contains_i64"
                    }
                    DataType::List => "rt_lists_contains_i64",
                    _ => "rt_lists_contains_i64",
                };
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::Global(rt_name.to_string()),
                        vec![haystack_val, needle_val],
                        MirType {
                            data_type: DataType::Bool,
                        },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call { name, args, .. } if name == "lists.map" && args.len() == 2 => {
                self.lower_lists_map(args)
            }
            Expression::Call { name, args, .. } if name == "lists.filter" && args.len() == 2 => {
                self.lower_lists_filter(args)
            }
            Expression::Call { name, args, .. } if name == "lists.fold" && args.len() == 3 => {
                self.lower_lists_fold(args)
            }
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } => {
                let is_instance_method = name.contains('.')
                    && !name.contains("::")
                    && name
                        .split_once('.')
                        .map(|(prefix, _method)| self.var_types.contains_key(prefix))
                        .unwrap_or(false);
                let (resolved_name, mir_args) = if is_instance_method {
                    let (prefix, method) = name.split_once('.').unwrap();
                    let var_ty = self.var_types.get(prefix).unwrap().clone();
                    let struct_name = match &var_ty {
                        DataType::StructNamed(s) => s.clone(),
                        _ => String::new(),
                    };
                    let norm = struct_name
                        .split_once('[')
                        .map(|(b, _)| b.to_string())
                        .unwrap_or(struct_name);
                    let qualified = self
                        .method_map
                        .get(&norm)
                        .and_then(|methods| methods.get(method))
                        .cloned();
                    match qualified {
                        Some(qn) => {
                            let receiver = self.lower_expression(&Expression::Identifier(
                                crate::parser::ast::Identifier {
                                    name: prefix.to_string(),
                                    data_type: DataType::Unknown,
                                    line: 0,
                                    column: 0,
                                },
                            ));
                            let mut instance_args = vec![receiver];
                            instance_args.extend(self.lower_call_args(name, args));
                            (qn, instance_args)
                        }
                        None => {
                            let mir_args = self.lower_call_args(name, args);
                            let resolved = self
                                .bare_to_qualified
                                .get(name.as_str())
                                .cloned()
                                .unwrap_or_else(|| name.clone());
                            (resolved, mir_args)
                        }
                    }
                } else {
                    let mir_args = self.lower_call_args(name, args);
                    let resolved = self
                        .bare_to_qualified
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    (resolved, mir_args)
                };

                let is_closure_var = self.var_types.get(name)
                    .map(|ty| matches!(ty, DataType::Closure { .. } | DataType::Function))
                    .unwrap_or(false);

                let callee = if is_closure_var {
                    if let Some(&ptr) = self.vars.get(name) {
                        MirValue::Temp(ptr)
                    } else {
                        MirValue::FunctionRef {
                            name: resolved_name,
                            env: Box::new(MirValue::Const(MirConst::None)),
                        }
                    }
                } else {
                    MirValue::FunctionRef {
                        name: resolved_name,
                        env: Box::new(MirValue::Const(MirConst::None)),
                    }
                };

                let last = self.current_block;
                if matches!(data_type, DataType::None) {
                    self.func.blocks[last].push(
                        None,
                        MirOp::Call(
                            callee,
                            mir_args,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::Const(MirConst::None)
                } else {
                    let result = self.new_temp();
                    self.func.blocks[last].push(
                        Some(result),
                        MirOp::Call(
                            callee,
                            mir_args,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(result)
                }
            }
            Expression::UseMacro { inner } => self.lower_expression(inner),
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => self.lower_capturing_closure(params, body, return_type, capture),
            Expression::Match {
                value,
                cases,
                default,
                data_type,
                ..
            } => {
                let match_val = self.lower_expression(value);
                let result_ptr = self.new_temp();
                let result_type = MirType {
                    data_type: data_type.clone(),
                };
                let initial_block = self.current_block;

                self.func.blocks[initial_block].push(
                    Some(result_ptr),
                    MirOp::Alloca(result_type.clone()),
                    loc,
                );

                let n = cases.len();
                let mut chk_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    chk_blocks.push(self.new_block(&format!("match_chk_{}", i)));
                }

                let first_chk = chk_blocks[0];
                let chk_base = first_chk;
                let case_base = first_chk + n;
                let default_idx = case_base + n;
                let end_idx = default_idx + 1;

                self.func.blocks[initial_block].terminator = MirTerminator::Br(first_chk);

                for (i, (pattern, _body)) in cases.iter().enumerate() {
                    let chk = chk_blocks[i];
                    let cs = case_base + i;
                    let next = if i + 1 < n {
                        chk_base + i + 1
                    } else {
                        default_idx
                    };

                    match pattern {
                        Expression::Literal(lit) => {
                            let lit_val = self.lower_literal(lit);
                            let cmp = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(cmp),
                                MirOp::ICmp(MirCmp::Eq, match_val.clone(), lit_val),
                                loc,
                            );
                            self.func.blocks[chk].terminator =
                                MirTerminator::BrCond(MirValue::temp(cmp), cs, next);
                        }
                        Expression::EnumVariant {
                            enum_name,
                            variant_name,
                            ..
                        }
                        | Expression::EnumVariantPath {
                            enum_name,
                            variant_name,
                            ..
                        } => {
                            let discriminant = self
                                .enum_types
                                .get(enum_name)
                                .and_then(|variants| {
                                    variants
                                        .iter()
                                        .find(|(n, _)| n == variant_name)
                                        .map(|(_, idx)| *idx as i64)
                                })
                                .unwrap_or(0);
                            let cmp = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(cmp),
                                MirOp::ICmp(
                                    MirCmp::Eq,
                                    match_val.clone(),
                                    MirValue::Const(MirConst::Int(discriminant)),
                                ),
                                loc,
                            );
                            self.func.blocks[chk].terminator =
                                MirTerminator::BrCond(MirValue::temp(cmp), cs, next);
                        }
                        _ => {
                            self.func.blocks[chk].terminator = MirTerminator::Br(cs);
                        }
                    }
                }

                for (i, (_pattern, body)) in cases.iter().enumerate() {
                    let cs = self.new_block(&format!("match_case_{}", i));
                    self.current_block = cs;
                    let body_val = self.lower_expression(body);
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(result_ptr), body_val),
                        loc,
                    );
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                let default_block = self.new_block("match_default");
                self.current_block = default_block;
                {
                    let default_val = self.lower_expression(default);
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(result_ptr), default_val),
                        loc,
                    );
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                let end_block = self.new_block("match_end");
                self.current_block = end_block;
                let loaded = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(loaded),
                    MirOp::Load(MirValue::temp(result_ptr), result_type),
                    loc,
                );
                MirValue::temp(loaded)
            }
            Expression::NamedArg { value, .. } => self.lower_expression(value),
            Expression::MemberAccess {
                target,
                member,
                data_type,
            } => {
                let struct_name = self.get_struct_name(target);
                if let Some(struct_name) = struct_name {
                    let norm_name = struct_name
                        .split_once('[')
                        .map(|(base, _)| base.to_string())
                        .unwrap_or_else(|| struct_name.clone());
                    if let Some(fields) = self.struct_types.get(&norm_name)
                        && let Some(field_index) =
                            fields.iter().position(|(name, _)| name == member)
                    {
                        let actual_field_type = fields[field_index].1.clone();
                        let target_val = self.lower_expression(target);
                        let last = self.current_block;
                        let gep_result = self.new_temp();
                        self.func.blocks[last].push(
                            Some(gep_result),
                            MirOp::Gep(
                                target_val,
                                vec![
                                    MirValue::Const(MirConst::Int(0)),
                                    MirValue::Const(MirConst::Int(field_index as i64)),
                                ],
                                norm_name.clone(),
                            ),
                            loc,
                        );
                        if matches!(actual_field_type, DataType::Array { .. }) {
                            return MirValue::temp(gep_result);
                        }
                        let load_result = self.new_temp();
                        self.func.blocks[last].push(
                            Some(load_result),
                            MirOp::Load(
                                MirValue::temp(gep_result),
                                MirType {
                                    data_type: actual_field_type.clone(),
                                },
                            ),
                            loc,
                        );
                        if *data_type != actual_field_type {
                            return self.emit_convert(
                                MirValue::temp(load_result),
                                &actual_field_type,
                                data_type,
                                loc,
                            );
                        }
                        return MirValue::temp(load_result);
                    }
                }
                MirValue::Const(MirConst::None)
            }
            Expression::Tuple {
                elements,
                data_type,
            } => match data_type {
                DataType::StructNamed(name) => {
                    let mir_args: Vec<MirValue> =
                        elements.iter().map(|e| self.lower_expression(e)).collect();
                    let result = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(result),
                        MirOp::Call(
                            MirValue::FunctionRef {
                                name: name.clone(),
                                env: Box::new(MirValue::Const(MirConst::None)),
                            },
                            mir_args,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(result)
                }
                _ => MirValue::Const(MirConst::None),
            },

            Expression::Index {
                target,
                index,
                data_type,
            } => {
                if let Some(value) = lower_index_read(self, target, index, data_type) {
                    return value;
                }
                let target_val = self.lower_expression(target);
                let index_val = self.lower_expression(index);
                let target_type = extract_data_type(target);
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
                let elem_llvm = llvm_elem_type_str(data_type);
                let adjusted_index = if matches!(
                    target_type,
                    DataType::Vector { .. } | DataType::List
                ) {
                    let elem_size = llvm_type_byte_size(&elem_llvm);
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
                    MirOp::Gep(target_val, vec![adjusted_index], elem_llvm),
                    loc,
                );
                let loaded = self.new_temp();
                self.func.blocks[last].push(
                    Some(loaded),
                    MirOp::Load(
                        MirValue::temp(gep),
                        MirType {
                            data_type: data_type.clone(),
                        },
                    ),
                    loc,
                );
                MirValue::temp(loaded)
            }
            Expression::Reference { expr, .. } => {
                if let Expression::Identifier(id) = expr.as_ref() {
                    if let Some(&ptr) = self.vars.get(&id.name) {
                        MirValue::temp(ptr)
                    } else {
                        MirValue::Const(MirConst::None)
                    }
                } else {
                    MirValue::Const(MirConst::None)
                }
            }
            Expression::Dereference { expr, data_type } => {
                let ptr_val = self.lower_expression(expr);
                let source_type = extract_data_type(expr);
                if is_trivial_deref(&source_type, data_type) {
                    ptr_val
                } else {
                    let loaded = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(loaded),
                        MirOp::Load(
                            ptr_val,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(loaded)
                }
            }
            Expression::UnaryOp {
                operator, operand, ..
            } => {
                let op_val = self.lower_expression(operand);
                let result = self.new_temp();
                let last = self.current_block;
                match operator.as_str() {
                    "-" => {
                        let zero = MirValue::Const(MirConst::Int(0));
                        self.func.blocks[last].push(Some(result), MirOp::Sub(zero, op_val), loc);
                    }
                    "!" => {
                        let zero = MirValue::Const(MirConst::Bool(false));
                        self.func.blocks[last].push(
                            Some(result),
                            MirOp::ICmp(MirCmp::Eq, op_val, zero),
                            loc,
                        );
                    }
                    _ => {}
                }
                MirValue::temp(result)
            }
            Expression::List {
                elements,
                element_type: _,
                data_type,
            } => match data_type {
                DataType::Array { element_type, .. } => {
                    let last = self.current_block;
                    let arr_ptr = self.new_temp();
                    self.func.blocks[last].push(
                        Some(arr_ptr),
                        MirOp::Alloca(MirType {
                            data_type: data_type.clone(),
                        }),
                        loc,
                    );
                    let elem_llvm = llvm_elem_type_str(element_type);
                    for (i, elem) in elements.iter().enumerate() {
                        let elem_val = self.lower_expression(elem);
                        let last = self.current_block;
                        let gep = self.new_temp();
                        self.func.blocks[last].push(
                            Some(gep),
                            MirOp::Gep(
                                MirValue::temp(arr_ptr),
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
                    MirValue::temp(arr_ptr)
                }
                DataType::Vector { element_type, .. } => {
                    let last = self.current_block;
                    let list_ptr = self.new_temp();
                    self.func.blocks[last].push(
                        Some(list_ptr),
                        MirOp::Alloca(MirType {
                            data_type: DataType::Unknown,
                        }),
                        loc,
                    );
                    let init = self.new_temp();
                    self.func.blocks[last].push(
                        Some(init),
                        MirOp::Call(
                            MirValue::Global("rt_list_create".to_string()),
                            vec![
                                MirValue::Const(MirConst::Int(4)),
                                MirValue::Const(MirConst::Int(8)),
                            ],
                            MirType {
                                data_type: DataType::Unknown,
                            },
                        ),
                        loc,
                    );
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        None,
                        MirOp::Store(MirValue::temp(list_ptr), MirValue::temp(init)),
                        loc,
                    );
                    let push_fn = match element_type.as_ref() {
                        DataType::I64 | DataType::U64 | DataType::Char => "rt_list_push_i64",
                        _ => "rt_list_push_ptr",
                    };
                    for elem in elements {
                        let elem_val = self.lower_expression(elem);
                        let last = self.current_block;
                        let loaded = self.new_temp();
                        self.func.blocks[last].push(
                            Some(loaded),
                            MirOp::Load(
                                MirValue::temp(list_ptr),
                                MirType {
                                    data_type: DataType::Unknown,
                                },
                            ),
                            loc,
                        );
                        let pushed = self.new_temp();
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            Some(pushed),
                            MirOp::Call(
                                MirValue::Global(push_fn.to_string()),
                                vec![MirValue::temp(loaded), elem_val],
                                MirType {
                                    data_type: DataType::Unknown,
                                },
                            ),
                            loc,
                        );
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            None,
                            MirOp::Store(MirValue::temp(list_ptr), MirValue::temp(pushed)),
                            loc,
                        );
                    }
                    let last = self.current_block;
                    let final_list = self.new_temp();
                    self.func.blocks[last].push(
                        Some(final_list),
                        MirOp::Load(
                            MirValue::temp(list_ptr),
                            MirType {
                                data_type: DataType::Unknown,
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(final_list)
                }
                _ => MirValue::Const(MirConst::None),
            },
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                ..
            } => {
                let discriminant = self
                    .enum_types
                    .get(enum_name)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .find(|(n, _)| n == variant_name)
                            .map(|(_, idx)| *idx as i64)
                    })
                    .unwrap_or(0);
                MirValue::Const(MirConst::Int(discriminant))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                ..
            } => {
                let discriminant = self
                    .enum_types
                    .get(enum_name)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .find(|(n, _)| n == variant_name)
                            .map(|(_, idx)| *idx as i64)
                    })
                    .unwrap_or(0);
                MirValue::Const(MirConst::Int(discriminant))
            }
            Expression::Dict {
                entries, data_type, ..
            } => {
                let vt = match data_type {
                    DataType::Map { value_type, .. } => value_type.as_ref(),
                    _ => &DataType::Unknown,
                };
                let last = self.current_block;
                let dict_ptr = self.new_temp();
                self.func.blocks[last].push(
                    Some(dict_ptr),
                    MirOp::Alloca(MirType {
                        data_type: DataType::Unknown,
                    }),
                    loc,
                );
                let last = self.current_block;
                self.func.blocks[last].push(
                    None,
                    MirOp::Store(MirValue::temp(dict_ptr), MirValue::Const(MirConst::None)),
                    loc,
                );
                for (key_expr, val_expr) in entries {
                    let key_val = self.lower_expression(key_expr);
                    let val_val = self.lower_expression(val_expr);
                    let last = self.current_block;
                    let cur_dict = self.new_temp();
                    self.func.blocks[last].push(
                        Some(cur_dict),
                        MirOp::Load(
                            MirValue::temp(dict_ptr),
                            MirType {
                                data_type: DataType::Unknown,
                            },
                        ),
                        loc,
                    );
                    let is_scalar = vt == &DataType::I64
                        || vt == &DataType::I128
                        || vt == &DataType::U64
                        || vt == &DataType::U128
                        || vt == &DataType::Char
                        || vt == &DataType::Bool
                        || vt == &DataType::I32
                        || vt == &DataType::U32;
                    let set_fn = if is_scalar {
                        "rt_dicts_set_i64"
                    } else {
                        "rt_dicts_set_with_kind"
                    };
                    let mut call_args = vec![MirValue::temp(cur_dict), key_val, val_val];
                    if !is_scalar {
                        call_args.push(MirValue::Const(MirConst::Int(data_type_to_kind(vt))));
                    }
                    let pushed = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(pushed),
                        MirOp::Call(
                            MirValue::Global(set_fn.to_string()),
                            call_args,
                            MirType {
                                data_type: DataType::Unknown,
                            },
                        ),
                        loc,
                    );
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        None,
                        MirOp::Store(MirValue::temp(dict_ptr), MirValue::temp(pushed)),
                        loc,
                    );
                }
                let last = self.current_block;
                let final_dict = self.new_temp();
                self.func.blocks[last].push(
                    Some(final_dict),
                    MirOp::Load(
                        MirValue::temp(dict_ptr),
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    ),
                    loc,
                );
                MirValue::temp(final_dict)
            }
            _ => MirValue::Const(MirConst::None),
        }
    }

    pub(crate) fn extract_closure_expr(expr: &Expression) -> &Expression {
        if let Expression::Closure { body, .. } = expr
            && let Some(Statement::Return(Some(inner))) = body.first()
        {
            return inner;
        }
        expr
    }

    pub(crate) fn lower_lists_map(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::Unknown,
            }),
            loc,
        );
        let init = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(init),
            MirOp::Call(
                MirValue::Global("rt_list_create".to_string()),
                vec![
                    MirValue::Const(MirConst::Int(4)),
                    MirValue::Const(MirConst::Int(8)),
                ],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(init)),
            loc,
        );

        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("map_cond");
        let body_block = self.new_block("map_body");
        let end_block = self.new_block("map_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(
                MirValue::temp(i_ptr),
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
                vec![list_val.clone()],
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
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let mapped = self.new_temp();
        self.func.blocks[body_block].push(
            Some(mapped),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(elem)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[body_block].push(
            Some(loaded_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[body_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(mapped)],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(pushed)),
            loc,
        );
        let old_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(old_i),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[body_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        MirValue::temp(final_result)
    }

    pub(crate) fn lower_lists_filter(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::Unknown,
            }),
            loc,
        );
        let init = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(init),
            MirOp::Call(
                MirValue::Global("rt_list_create".to_string()),
                vec![
                    MirValue::Const(MirConst::Int(4)),
                    MirValue::Const(MirConst::Int(8)),
                ],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(init)),
            loc,
        );

        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );
        let elem_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(elem_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("filter_cond");
        let body_block = self.new_block("filter_body");
        let keep_block = self.new_block("filter_keep");
        let inc_block = self.new_block("filter_inc");
        let end_block = self.new_block("filter_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(
                MirValue::temp(i_ptr),
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
                vec![list_val.clone()],
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
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem_raw = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem_raw),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(elem_ptr), MirValue::temp(elem_raw)),
            loc,
        );
        let keep = self.new_temp();
        self.func.blocks[body_block].push(
            Some(keep),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(elem_raw)],
                MirType {
                    data_type: DataType::Bool,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].terminator =
            MirTerminator::BrCond(MirValue::temp(keep), keep_block, inc_block);

        self.current_block = keep_block;
        let elem = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(elem),
            MirOp::Load(
                MirValue::temp(elem_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(loaded_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(elem)],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        self.func.blocks[keep_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(pushed)),
            loc,
        );
        self.func.blocks[keep_block].terminator = MirTerminator::Br(inc_block);

        self.current_block = inc_block;
        let old_i = self.new_temp();
        self.func.blocks[inc_block].push(
            Some(old_i),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[inc_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[inc_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[inc_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        MirValue::temp(final_result)
    }

    pub(crate) fn lower_lists_fold(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let acc_init = self.lower_expression(&args[0]);
        let closure_val = self.lower_expression(&args[1]);
        let list_val = self.lower_expression(&args[2]);

        let acc_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(acc_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(acc_ptr), acc_init),
            loc,
        );

        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("fold_cond");
        let body_block = self.new_block("fold_body");
        let end_block = self.new_block("fold_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(
                MirValue::temp(i_ptr),
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
                vec![list_val.clone()],
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
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let acc_loaded = self.new_temp();
        self.func.blocks[body_block].push(
            Some(acc_loaded),
            MirOp::Load(
                MirValue::temp(acc_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let new_acc = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_acc),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(acc_loaded), MirValue::temp(elem)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(acc_ptr), MirValue::temp(new_acc)),
            loc,
        );
        let old_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(old_i),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[body_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(
                MirValue::temp(acc_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        MirValue::temp(final_result)
    }

    pub(crate) fn get_target_elem_type(&self, target: &Expression) -> String {
        if let Expression::Identifier(id) = target
            && let Some(ty) = self.var_types.get(&id.name)
        {
            match ty {
                DataType::Array { element_type, .. }
                | DataType::Vector { element_type, .. }
                | DataType::Slice { element_type, .. } => {
                    return llvm_elem_type_str(element_type);
                }
                _ => {}
            }
        }
        "i64".to_string()
    }

    pub(crate) fn get_struct_name(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(id) => self.var_types.get(&id.name).and_then(|t| match t {
                DataType::StructNamed(name) => Some(name.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    pub(crate) fn lower_literal(&self, lit: &Literal) -> MirValue {
        match lit {
            Literal::Int(v) => MirValue::Const(MirConst::Int(*v)),
            Literal::Float(v) => MirValue::Const(MirConst::Float(*v)),
            Literal::Bool(v) => MirValue::Const(MirConst::Bool(*v)),
            Literal::Char(v) => MirValue::Const(MirConst::Char(char::from_u32(*v).unwrap_or('\0'))),
            Literal::Str(v) => MirValue::Const(MirConst::Str(v.clone())),
            Literal::None => MirValue::Const(MirConst::None),
            _ => MirValue::Const(MirConst::None),
        }
    }

    pub(crate) fn emit_convert(
        &mut self,
        src_val: MirValue,
        src_type: &DataType,
        target_type: &DataType,
        loc: (usize, usize),
    ) -> MirValue {
        if src_type == target_type || *target_type == DataType::Unknown {
            return src_val;
        }
        use DataType::*;
        let op = match (src_type, target_type) {
            // Entero -> Flotante (con signo)
            (
                I64 | I128 | I32 | I16 | I8 | Char,
                F64 | F32,
            ) => MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() }),
            // Flotante -> Entero (truncamiento de fracción)
            (
                F64 | F32,
                I64 | I128 | I32 | I16 | I8 | U64 | U128 | U32 | U16 | U8 | Char,
            ) => MirOp::Fptosi(src_val, MirType { data_type: target_type.clone() }),
            // Flotante -> Flotante (cambio de ancho)
            (F64, F32) => MirOp::Fptrunc(src_val, MirType { data_type: target_type.clone() }),
            (F32, F64) => MirOp::Fpext(src_val, MirType { data_type: target_type.clone() }),
            // Entero -> Entero de distinto ancho / signo
            (s, t) if is_int_or_char(s) && is_int_or_char(t) => {
                let s_w = int_width(s);
                let t_w = int_width(t);
                if t_w >= s_w {
                    // Extensión: con signo para tipos signed, sin signo para unsigned.
                    if is_signed_int(s) {
                        MirOp::SExt(src_val, MirType { data_type: target_type.clone() })
                    } else {
                        MirOp::ZExt(src_val, MirType { data_type: target_type.clone() })
                    }
                } else {
                    MirOp::Trunc(src_val, MirType { data_type: target_type.clone() })
                }
            }
            _ => MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() }),
        };
        let result = self.new_temp();
        let last = self.current_block;
        self.func.blocks[last].push(Some(result), op, loc);
        MirValue::temp(result)
    }
}

fn is_int_or_char(t: &DataType) -> bool {
    matches!(
        t,
        DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::I128
            | DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::U128
            | DataType::Char
    )
}

fn is_signed_int(t: &DataType) -> bool {
    matches!(
        t,
        DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64 | DataType::I128 | DataType::Char
    )
}

fn int_width(t: &DataType) -> u32 {
    match t {
        DataType::I8 | DataType::U8 => 8,
        DataType::I16 | DataType::U16 => 16,
        DataType::I32 | DataType::U32 => 32,
        DataType::I64 | DataType::U64 => 64,
        DataType::I128 | DataType::U128 => 128,
        DataType::Char => 32,
        _ => 64,
    }
}
