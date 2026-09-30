use super::MirLower;
use super::collections::lower_index_read;
use super::stmt::is_owned_ptr_type;
use super::types::{
    data_type_to_kind, extract_data_type, is_map_or_dict_type, is_trivial_deref,
    llvm_elem_type_str, llvm_type_byte_size,
};
use crate::compiler::location::expression_location;
use crate::compiler::mir::*;
use crate::parser::ast::{DataType, Expression, Literal};

fn is_float_dt(t: &DataType) -> bool {
    matches!(t, DataType::F32 | DataType::F64)
}

/// Returns true if both operands of a binary / or % are integer types
/// (i.e. the operation should be lowered with a div-by-zero check instead of fdiv/frem).
fn is_int_division(left: &Expression, right: &Expression) -> bool {
    let lt = extract_data_type(left);
    let rt = extract_data_type(right);
    !is_float_dt(&lt) && !is_float_dt(&rt)
}

/// True when `expr` evaluates to a freshly allocated managed pointer that this
/// evaluation owns (a string-returning call). Variable/field reads are borrows
/// and array element reads do not retain, so neither is included.
fn concat_operand_is_owned(expr: &Expression) -> bool {
    match expr {
        Expression::Call { data_type, .. } => is_owned_ptr_type(data_type),
        Expression::Ascription { expr, .. } => concat_operand_is_owned(expr),
        _ => false,
    }
}

/// Collects all operands in a string concatenation chain.
/// For `a + b + c + d`, this will return [a, b, c, d] in order.
/// Owned operands (fresh allocations used only by the concat) are recorded in
/// `to_free` so the caller can release them after `Concat` copies their bytes.
fn collect_concat_operands(
    lower: &mut MirLower,
    expr: &Expression,
    operands: &mut Vec<MirValue>,
    to_free: &mut Vec<MirValue>,
) {
    if let Expression::BinaryOp {
        operator,
        left,
        right,
        ..
    } = expr
        && operator == "+" {
            let left_ty = extract_data_type(left);
            let right_ty = extract_data_type(right);
            if left_ty == DataType::Str && right_ty == DataType::Str {
                // Recursively collect operands from left and right
                collect_concat_operands(lower, left, operands, to_free);
                collect_concat_operands(lower, right, operands, to_free);
                return;
            }
        }
    // Not a string concat, add the expression as a single operand
    let value = lower.lower_expression(expr);
    if concat_operand_is_owned(expr) && matches!(value, MirValue::Temp(_)) {
        to_free.push(value.clone());
    }
    operands.push(value);
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
                    self.materialize_array_value(
                        lowered,
                        &arg_type,
                        expression_location(a).to_tuple(),
                    )
                }
            })
            .collect()
    }

    /// Array expressions lower to a POINTER to the array in memory, but function
    /// signatures take arrays by value (`[N x T]`). Materialize the value with a
    /// Load so the caller passes a by-value array matching the callee's signature.
    pub(crate) fn materialize_array_value(
        &mut self,
        v: MirValue,
        ty: &DataType,
        loc: (usize, usize),
    ) -> MirValue {
        if !matches!(ty, DataType::Array { .. }) {
            return v;
        }
        let is_call_result = if let MirValue::Temp(id) = v {
            self.func.blocks[self.current_block]
                .insts
                .last()
                .is_some_and(|inst| inst.result == Some(id) && matches!(inst.op, MirOp::Call(..)))
        } else {
            false
        };
        if is_call_result {
            return v;
        }
        let loaded = self.new_temp();
        let last = self.current_block;
        self.func.blocks[last].push(
            Some(loaded),
            MirOp::Load(
                v,
                MirType {
                    data_type: ty.clone(),
                },
            ),
            loc,
        );
        MirValue::temp(loaded)
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
            Expression::Literal { lit, .. } => {
                let expr_ty = extract_data_type(expr);
                // Special handling for None literal when target is Maybe[T]
                if matches!(lit, Literal::None) && matches!(&expr_ty, DataType::Maybe { inner: _ }) {
                    // Unboxed Maybe: { i1 tag, T value } with tag = 0
                    // Allocate struct (zero-initialized = {i1 0, T zero}), then load
                    let _inner_ty = match &expr_ty {
                        DataType::Maybe { inner } => *inner.clone(),
                        _ => DataType::Unknown,
                    };
                    let struct_ty = MirType {
                        data_type: expr_ty.clone(),
                    };
                    let ptr = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(ptr),
                        MirOp::Alloca(struct_ty.clone()),
                        loc,
                    );
                    let loaded = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(loaded),
                        MirOp::Load(MirValue::temp(ptr), struct_ty.clone()),
                        loc,
                    );
                    return MirValue::temp(loaded);
                }
                let val = self.lower_literal(lit);
                let natural = crate::types::unify::literal_type(lit);
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
                // String concatenation: build the whole flattened operand chain
                // lazily. Lowering left/right eagerly below would evaluate every
                // operand a second time (dead temporaries) and leak intermediate
                // strings; the flattened `Concat` is the single evaluation point.
                if operator == "+"
                    && extract_data_type(left) == DataType::Str
                    && extract_data_type(right) == DataType::Str
                {
                    let result = self.new_temp();
                    let mut operands = Vec::new();
                    let mut to_free = Vec::new();
                    collect_concat_operands(self, left, &mut operands, &mut to_free);
                    collect_concat_operands(self, right, &mut operands, &mut to_free);
                    let last = self.current_block;
                    self.func.blocks[last].push(Some(result), MirOp::Concat(operands), loc);
                    // `Concat` copies the operand bytes, so fresh operand
                    // allocations can be released immediately.
                    for operand in to_free {
                        let last = self.current_block;
                        self.func.blocks[last].push(None, MirOp::Drop(operand), loc);
                    }
                    return MirValue::temp(result);
                }

                let l = self.lower_expression(left);
                let r = self.lower_expression(right);

                // Inline integer division/remainder: check div-by-zero + overflow at MIR level
                // instead of calling rt_div_i64/rt_rem_i64. Float division is untouched.
                if (operator == "/" || operator == "%") && is_int_division(left, right) {
                    let pre_check = self.current_block;

                    // Panic message (resolved from the operator)
                    let panic_msg = if operator == "/" {
                        "division by zero"
                    } else {
                        "remainder by zero"
                    };

                    // Create blocks: div_ok, div_panic, div_cont
                    let ok_block = self.new_block("div_ok");
                    let panic_block = self.new_block("div_panic");
                    let cont_block = self.new_block("div_cont");

                    // Check: r == 0
                    let is_zero = self.new_temp();
                    self.func.blocks[pre_check].push(
                        Some(is_zero),
                        MirOp::ICmp(MirCmp::Eq, r.clone(), MirValue::Const(MirConst::Int(0))),
                        loc,
                    );
                    self.func.blocks[pre_check].terminator =
                        MirTerminator::BrCond(MirValue::temp(is_zero), panic_block, ok_block);

                    // Panic block: call rt_panic_loc then unreachable
                    self.current_block = panic_block;
                    self.func.blocks[panic_block].push(
                        None,
                        MirOp::Call(
                            MirValue::Global("rt_panic_loc".to_string()),
                            vec![
                                MirValue::Const(MirConst::Str(panic_msg.to_string())),
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
                    self.func.blocks[panic_block].terminator = MirTerminator::Unreachable;

                    // OK block: native sdiv/srem
                    self.current_block = ok_block;
                    let result = self.new_temp();
                    let op = if operator == "/" {
                        MirOp::SDiv(l, r)
                    } else {
                        MirOp::SRem(l, r)
                    };
                    self.func.blocks[ok_block].push(Some(result), op, loc);
                    self.func.blocks[ok_block].terminator = MirTerminator::Br(cont_block);

                    // Continue block: result is available here
                    self.current_block = cont_block;
                    return MirValue::temp(result);
                }

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
                    if *data_type != DataType::Unknown
                        && super::stmt::needs_convert(&natural, data_type)
                    {
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
            // bits::<T>(value) — bit-level reinterpretation between same-width
            // scalars. typeck has already validated the widths, so this lowers
            // straight to MirOp::BitCast (no runtime call, no builtin).
            Expression::Call { name, args, .. } if crate::bitcast_target(name).is_some() => {
                let target = crate::bitcast_target(name).unwrap_or(DataType::Unknown);
                let source = self.lower_expression(&args[0]);
                let dst_type = MirType {
                    data_type: target,
                };
                let result = self.new_temp();
                let block = self.current_block;
                self.func.blocks[block].push(
                    Some(result),
                    MirOp::BitCast(source, dst_type),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call { name, args, .. } if name == "range" => {
                let mir_args: Vec<MirValue> =
                    args.iter().map(|a| self.lower_expression(a)).collect();
                let result = self.new_temp();
                let last = self.current_block;
                let callee = match mir_args.len() {
                    3 => "rt_math_range_step_i64",
                    2 => "rt_math_range_between_i64",
                    _ => "rt_math_range_i64",
                };
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::Global(callee.to_string()),
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
                // Constant folding: if len() is called on a string literal, emit the length directly.
                if let MirValue::Const(MirConst::Str(s)) = &arg_val
                    && matches!(
                        arg_type,
                        DataType::Str | DataType::Ref { .. } | DataType::RefMut { .. }
                    ) {
                        let len = s.len() as i64;
                        let result = self.new_temp();
                        self.func.blocks[self.current_block].push(
                            Some(result),
                            MirOp::Copy(MirValue::Const(MirConst::Int(len))),
                            loc,
                        );
                        return MirValue::temp(result);
                    }
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
                            DataType::I64
                                | DataType::I128
                                | DataType::U128
                                | DataType::Unknown
                                | DataType::Anything
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
                        DataType::StructNamed(s) | DataType::EnumNamed(s) => s.clone(),
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

                // Handle Maybe[T] builtin methods as inline struct operations
                if matches!(
                    resolved_name.as_str(),
                    "maybe.is_some"
                        | "maybe.is_none"
                        | "maybe.unwrap.i64"
                        | "maybe.unwrap.str"
                        | "maybe.unwrap.f64"
                        | "maybe.unwrap.ptr"
                        | "maybe.unwrap_or.i64"
                        | "maybe.unwrap_or.str"
                        | "maybe.unwrap_or.f64"
                        | "maybe.unwrap_or.ptr"
                ) {
                    return self.lower_maybe_builtin(&resolved_name, mir_args, data_type, loc);
                }

                let is_closure_var = self
                    .var_types
                    .get(name)
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
            Expression::MacroCall { inner } => self.lower_expression(inner),
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

                // Allocate every block up front so that case bodies (which may
                // create their own blocks via if/while/for/nested match) never
                // shift the fixed block indices the chk chain points at.
                let mut chk_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    chk_blocks.push(self.new_block(&format!("match_chk_{}", i)));
                }
                let mut case_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    case_blocks.push(self.new_block(&format!("match_case_{}", i)));
                }
                let default_idx = self.new_block("match_default");
                let end_idx = self.new_block("match_end");

                self.func.blocks[initial_block].terminator =
                    MirTerminator::Br(if n > 0 { chk_blocks[0] } else { default_idx });

                for (i, (pattern, _body)) in cases.iter().enumerate() {
                    let chk = chk_blocks[i];
                    let cs = case_blocks[i];
                    let next = if i + 1 < n {
                        chk_blocks[i + 1]
                    } else {
                        default_idx
                    };

                    match pattern {
                        Expression::Literal { lit, .. } => {
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
                            let variant_full = format!("{}.{}", enum_name, variant_name);
                            let disc_gep = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(disc_gep),
                                MirOp::Gep(
                                    match_val.clone(),
                                    vec![
                                        MirValue::Const(MirConst::Int(0)),
                                        MirValue::Const(MirConst::Int(0)),
                                    ],
                                    variant_full.clone(),
                                ),
                                loc,
                            );
                            let disc = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(disc),
                                MirOp::Load(
                                    MirValue::temp(disc_gep),
                                    MirType {
                                        data_type: DataType::I64,
                                    },
                                ),
                                loc,
                            );
                            let cmp = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(cmp),
                                MirOp::ICmp(
                                    MirCmp::Eq,
                                    MirValue::temp(disc),
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

                for (i, (pattern, body)) in cases.iter().enumerate() {
                    let cs = case_blocks[i];
                    self.current_block = cs;
                    if let Expression::EnumVariant {
                        enum_name,
                        variant_name,
                        payloads,
                        ..
                    } = pattern
                    {
                        self.bind_match_payloads(
                            &match_val,
                            enum_name,
                            variant_name,
                            payloads,
                            loc,
                        );
                    }
                    let body_val = self.lower_expression(body);
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(result_ptr), body_val),
                        loc,
                    );
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                self.current_block = default_idx;
                {
                    let default_val = self.lower_expression(default);
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(result_ptr), default_val),
                        loc,
                    );
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                self.current_block = end_idx;
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
                        if matches!(actual_field_type, DataType::Array { .. } | DataType::StructNamed(_)) {
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
                    let norm = name
                        .split_once('[')
                        .map(|(base, _)| base.to_string())
                        .unwrap_or_else(|| name.clone());
                    let field_types: Vec<DataType> = self
                        .struct_types
                        .get(&norm)
                        .map(|fields| fields.iter().map(|(_, t)| t.clone()).collect())
                        .unwrap_or_default();
                    let mir_args: Vec<MirValue> = elements
                        .iter()
                        .enumerate()
                        .map(|(i, e)| {
                            let lowered = self.lower_expression(e);
                            let ft = field_types.get(i).cloned().unwrap_or(DataType::Unknown);
                            self.materialize_array_value(
                                lowered,
                                &ft,
                                expression_location(e).to_tuple(),
                            )
                        })
                        .collect();
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
                _ => { debug_assert!(false, "unhandled Expression variant in lowering"); MirValue::Const(MirConst::None) },
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

                // Determine the upper bound (array size constant or vec len)
                let upper_bound = match &target_type {
                    DataType::Array { size, .. } => MirValue::Const(MirConst::Int(*size as i64)),
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
                        MirValue::temp(len_val)
                    }
                    _ => {
                        // Non-indexable type; skip bounds check (code already type-checked)
                        MirValue::Const(MirConst::Int(0))
                    }
                };

                let need_check = matches!(
                    &target_type,
                    DataType::Array { .. } | DataType::Vector { .. } | DataType::List
                );

                if need_check {
                    let pre_block = self.current_block;

                    // Create blocks: check_upper (index < upper), bounds_panic, bounds_ok, bounds_cont
                    let check_upper_block = self.new_block("bounds_upper");
                    let panic_block = self.new_block("bounds_panic");
                    let ok_block = self.new_block("bounds_ok");
                    let cont_block = self.new_block("bounds_cont");

                    // Check 1: index < 0
                    let is_neg = self.new_temp();
                    self.func.blocks[pre_block].push(
                        Some(is_neg),
                        MirOp::ICmp(
                            MirCmp::Lt,
                            index_val.clone(),
                            MirValue::Const(MirConst::Int(0)),
                        ),
                        loc,
                    );
                    self.func.blocks[pre_block].terminator = MirTerminator::BrCond(
                        MirValue::temp(is_neg),
                        panic_block,
                        check_upper_block,
                    );

                    // Check 2: index >= upper_bound
                    self.current_block = check_upper_block;
                    let is_oob = self.new_temp();
                    self.func.blocks[check_upper_block].push(
                        Some(is_oob),
                        MirOp::ICmp(MirCmp::Ge, index_val.clone(), upper_bound),
                        loc,
                    );
                    self.func.blocks[check_upper_block].terminator =
                        MirTerminator::BrCond(MirValue::temp(is_oob), panic_block, ok_block);

                    // Panic block: call rt_panic_loc then unreachable
                    self.current_block = panic_block;
                    self.func.blocks[panic_block].push(
                        None,
                        MirOp::Call(
                            MirValue::Global("rt_panic_loc".to_string()),
                            vec![
                                MirValue::Const(MirConst::Str("index out of bounds".to_string())),
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
                    self.func.blocks[panic_block].terminator = MirTerminator::Unreachable;

                    // OK block: GEP + Load (the actual memory access)
                    self.current_block = ok_block;
                    let gep = self.new_temp();
                    let elem_llvm = llvm_elem_type_str(data_type);
                    let adjusted_index =
                        if matches!(target_type, DataType::Vector { .. } | DataType::List) {
                            let elem_size = llvm_type_byte_size(&elem_llvm);
                            let header_offset = 8 / elem_size;
                            let adj = self.new_temp();
                            self.func.blocks[ok_block].push(
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
                    self.func.blocks[ok_block].push(
                        Some(gep),
                        MirOp::Gep(target_val, vec![adjusted_index], elem_llvm),
                        loc,
                    );
                    let loaded = self.new_temp();
                    self.func.blocks[ok_block].push(
                        Some(loaded),
                        MirOp::Load(
                            MirValue::temp(gep),
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    self.func.blocks[ok_block].terminator = MirTerminator::Br(cont_block);

                    self.current_block = cont_block;
                    MirValue::temp(loaded)
                } else {
                    let gep = self.new_temp();
                    let elem_llvm = llvm_elem_type_str(data_type);
                    self.func.blocks[last].push(
                        Some(gep),
                        MirOp::Gep(target_val, vec![index_val], elem_llvm),
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
                    // Boolean vectors store 1 byte per element (bool is i1);
                    // every other element type keeps the 8-byte slot layout
                    // (i64/char via push_i64, pointers via push_ptr).
                    let elem_size = if matches!(element_type.as_ref(), DataType::Bool) {
                        1
                    } else {
                        8
                    };
                    self.func.blocks[last].push(
                        Some(init),
                        MirOp::Call(
                            MirValue::Global("rt_list_create".to_string()),
                            vec![
                                MirValue::Const(MirConst::Int(4)),
                                MirValue::Const(MirConst::Int(elem_size)),
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
                        DataType::Bool => "rt_list_push_scalar",
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
                        let mut push_args = vec![MirValue::temp(loaded), elem_val];
                        if push_fn == "rt_list_push_scalar" {
                            push_args.push(MirValue::Const(MirConst::Int(elem_size)));
                        }
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            Some(pushed),
                            MirOp::Call(
                                MirValue::Global(push_fn.to_string()),
                                push_args,
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
                _ => { debug_assert!(false, "unhandled Expression variant in lowering"); MirValue::Const(MirConst::None) },
            },
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
                ..
            } => {
                let variant_full = format!("{}.{}", enum_name, variant_name);
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::FunctionRef {
                            name: variant_full.clone(),
                            env: Box::new(MirValue::Const(MirConst::None)),
                        },
                        Vec::new(),
                        MirType {
                            data_type: data_type.clone(),
                        },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
                ..
            } => {
                let variant_full = format!("{}.{}", enum_name, variant_name);
                let payload_types: Vec<DataType> = self
                    .enum_payloads
                    .get(&variant_full)
                    .map(|fields| fields.iter().map(|(_, t)| t.clone()).collect())
                    .unwrap_or_default();
                let mir_args: Vec<MirValue> = payloads
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let lowered = self.lower_expression(p);
                        let pt = payload_types.get(i).cloned().unwrap_or(DataType::Unknown);
                        self.materialize_array_value(
                            lowered,
                            &pt,
                            expression_location(p).to_tuple(),
                        )
                    })
                    .collect();
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::FunctionRef {
                            name: variant_full.clone(),
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
                    let is_bool = vt == &DataType::Bool;
                    let set_fn = if is_bool {
                        "rt_dict_set_i64"
                    } else if is_scalar {
                        "rt_dicts_set_i64"
                    } else {
                        "rt_dicts_set_with_kind"
                    };
                    let mut call_args = vec![MirValue::temp(cur_dict), key_val.clone(), val_val];
                    if is_bool {
                        // Dict literal keys are always coerced to strings, so key
                        // kind is MIRE_KIND_STR (3) and key_i64 is 0. value_kind
                        // is MIRE_KIND_BOOL (2) → mire_kind_size = 1 → 1-byte slots.
                        call_args.insert(1, MirValue::Const(MirConst::Int(3)));
                        call_args.insert(2, MirValue::Const(MirConst::Int(data_type_to_kind(vt))));
                        call_args.insert(3, MirValue::Const(MirConst::Int(0)));
                    } else if !is_scalar {
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
            Expression::Some { value, data_type } => {
                // Unboxed Maybe: { i1 tag, T value }
                // tag = 1 for Some
                let lowered_value = self.lower_expression(value);
                let _inner_ty = match data_type {
                    DataType::Maybe { inner } => *inner.clone(),
                    _ => DataType::Unknown,
                };
                let struct_ty = MirType {
                    data_type: data_type.clone(),
                };
                // Allocate struct (zero-initialized = {i1 0, T zero})
                let ptr = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(ptr),
                    MirOp::Alloca(struct_ty.clone()),
                    loc,
                );
                // Load zero-initialized struct
                let base_struct = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(base_struct),
                    MirOp::Load(MirValue::temp(ptr), struct_ty.clone()),
                    loc,
                );
                // Insert tag = 1 at index 0
                let with_tag = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(with_tag),
                    MirOp::InsertValue(
                        MirValue::temp(base_struct),
                        MirValue::Const(MirConst::Int(1)),
                        vec![0],
                    ),
                    loc,
                );
                // Insert value at index 1
                let with_value = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(with_value),
                    MirOp::InsertValue(MirValue::temp(with_tag), lowered_value, vec![1]),
                    loc,
                );
                MirValue::temp(with_value)
            }
            Expression::Ok { value, data_type } => {
                let lowered_value = self.lower_expression(value);
                let result_ptr = self.new_temp();
                let alloc_ty = MirType {
                    data_type: DataType::Unknown,
                };
                self.func.blocks[self.current_block].push(
                    Some(result_ptr),
                    MirOp::Alloca(alloc_ty.clone()),
                    loc,
                );
                let op = if data_type == &DataType::I64 {
                    MirOp::Call(
                        MirValue::Global("rt_result_ok_i64".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                } else if data_type == &DataType::Str {
                    MirOp::Call(
                        MirValue::Global("rt_result_ok_str".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                } else {
                    MirOp::Call(
                        MirValue::Global("rt_result_ok_ptr".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                };
                let lowered_op = self.new_temp();
                self.func.blocks[self.current_block].push(Some(lowered_op), op, loc);
                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(lowered_op)),
                    loc,
                );
                let restored = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(restored),
                    MirOp::Load(MirValue::temp(result_ptr), alloc_ty.clone()),
                    loc,
                );
                MirValue::temp(restored)
            }
            Expression::Err { value, data_type } => {
                let lowered_value = self.lower_expression(value);
                let result_ptr = self.new_temp();
                let alloc_ty = MirType {
                    data_type: DataType::Unknown,
                };
                self.func.blocks[self.current_block].push(
                    Some(result_ptr),
                    MirOp::Alloca(alloc_ty.clone()),
                    loc,
                );
                let op = if data_type == &DataType::I64 {
                    MirOp::Call(
                        MirValue::Global("rt_result_err_i64".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                } else if data_type == &DataType::Str {
                    MirOp::Call(
                        MirValue::Global("rt_result_err_str".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                } else {
                    MirOp::Call(
                        MirValue::Global("rt_result_err_ptr".to_string()),
                        vec![lowered_value],
                        MirType {
                            data_type: DataType::Unknown,
                        },
                    )
                };
                let lowered_op = self.new_temp();
                self.func.blocks[self.current_block].push(Some(lowered_op), op, loc);
                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(lowered_op)),
                    loc,
                );
                let restored = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(restored),
                    MirOp::Load(MirValue::temp(result_ptr), alloc_ty.clone()),
                    loc,
                );
                MirValue::temp(restored)
            }
            Expression::Try {
                expr,
                data_type: result_type,
            } => {
                let operand = self.lower_expression(expr);
                let container_type = extract_data_type(expr);
                let is_result = matches!(&container_type, DataType::Result { .. });
                let inner_type = result_type.clone();

                let file = MirValue::Const(MirConst::Str(self.filename.clone()));
                let line = MirValue::Const(MirConst::Int(loc.0 as i64));
                let col = MirValue::Const(MirConst::Int(loc.1 as i64));

                let unwrap_block = self.new_block("try_unwrap");
                let err_block = self.new_block("try_err");
                let pre_check = self.current_block;

                if is_result {
                    // Result: use existing runtime functions
                    let check_fn = "rt_result_is_err";
                    let check_raw = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(check_raw),
                        MirOp::Call(
                            MirValue::Global(check_fn.to_string()),
                            vec![operand.clone()],
                            MirType {
                                data_type: DataType::I64,
                            },
                        ),
                        loc,
                    );
                    let is_err = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(is_err),
                        MirOp::ICmp(
                            MirCmp::Ne,
                            MirValue::temp(check_raw),
                            MirValue::Const(MirConst::Int(0)),
                        ),
                        loc,
                    );
                    self.func.blocks[pre_check].terminator =
                        MirTerminator::BrCond(MirValue::temp(is_err), err_block, unwrap_block);
                    self.current_block = err_block;
                    self.func.blocks[err_block].terminator =
                        MirTerminator::Ret(Some(operand.clone()));
                    self.current_block = unwrap_block;
                    let unwrap_fn = match &inner_type {
                        DataType::I64 => "rt_result_unwrap_i64",
                        DataType::Str => "rt_result_unwrap_str",
                        DataType::F64 => "rt_result_unwrap_f64",
                        _ => "rt_result_unwrap_ptr",
                    };
                    let unwrapped = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(unwrapped),
                        MirOp::Call(
                            MirValue::Global(unwrap_fn.to_string()),
                            vec![operand.clone(), line, col, file],
                            MirType {
                                data_type: inner_type,
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(unwrapped)
                } else {
                    // Maybe: unboxed struct { i1 tag, T value }
                    // Extract tag (index 0) and check if zero
                    let tag = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(tag),
                        MirOp::ExtractValue(MirValue::temp(pre_check), operand.clone(), vec![0]),
                        loc,
                    );
                    let is_none = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(is_none),
                        MirOp::ICmp(
                            MirCmp::Eq,
                            MirValue::temp(tag),
                            MirValue::Const(MirConst::Int(0)),
                        ),
                        loc,
                    );
                    self.func.blocks[pre_check].terminator =
                        MirTerminator::BrCond(MirValue::temp(is_none), err_block, unwrap_block);
                    self.current_block = err_block;
                    self.func.blocks[err_block].terminator =
                        MirTerminator::Ret(Some(operand.clone()));
                    self.current_block = unwrap_block;
                    // Extract value (index 1)
                    let value = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(value),
                        MirOp::ExtractValue(MirValue::temp(pre_check), operand.clone(), vec![1]),
                        loc,
                    );
                    MirValue::temp(value)
                }
            }
            Expression::Pipeline { input, stage, data_type, .. } => {
                let input_val = self.lower_expression(input);
                // Function/identifier stage: call with input as first argument
                let stage_val = self.lower_expression(stage);
                let result = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(result),
                    MirOp::Call(
                        stage_val,
                        vec![input_val],
                        MirType {
                            data_type: data_type.clone(),
                        },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            _ => { debug_assert!(false, "unhandled Expression variant in lowering"); MirValue::Const(MirConst::None) },
        }
    }

    /// Binds the payload identifiers of an enum variant pattern into the current
    /// block, loading each payload field out of the tagged match value before the
    /// case body is lowered. Must run with `self.current_block` set to the case block.
    pub(crate) fn bind_match_payloads(
        &mut self,
        match_val: &MirValue,
        enum_name: &str,
        variant_name: &str,
        payloads: &[Expression],
        loc: (usize, usize),
    ) {
        let variant_full = format!("{}.{}", enum_name, variant_name);
        let payload_types: Vec<DataType> = self
            .enum_payloads
            .get(&variant_full)
            .map(|fields| fields.iter().map(|(_, t)| t.clone()).collect())
            .unwrap_or_default();
        for (i, payload) in payloads.iter().enumerate() {
            let binding = match payload {
                Expression::Identifier(id) => id.name.clone(),
                Expression::NamedArg { name, .. } => name.clone(),
                _ => continue,
            };
            let ptype = payload_types.get(i).cloned().unwrap_or(DataType::Unknown);
            let slot = self.new_temp();
            self.func.blocks[self.current_block].push(
                Some(slot),
                MirOp::Alloca(MirType {
                    data_type: ptype.clone(),
                }),
                loc,
            );
            let gep = self.new_temp();
            self.func.blocks[self.current_block].push(
                Some(gep),
                MirOp::Gep(
                    match_val.clone(),
                    vec![
                        MirValue::Const(MirConst::Int(0)),
                        MirValue::Const(MirConst::Int(1 + i as i64)),
                    ],
                    variant_full.clone(),
                ),
                loc,
            );
            let loaded = self.new_temp();
            self.func.blocks[self.current_block].push(
                Some(loaded),
                MirOp::Load(
                    MirValue::temp(gep),
                    MirType {
                        data_type: ptype.clone(),
                    },
                ),
                loc,
            );
            self.func.blocks[self.current_block].push(
                None,
                MirOp::Store(MirValue::temp(slot), MirValue::temp(loaded)),
                loc,
            );
            self.vars.insert(binding.clone(), slot);
            self.var_types.insert(binding.clone(), ptype);
        }
    }

    /// Lower Maybe[T] builtin methods as inline struct operations.
    /// Unboxed representation: { i1 tag, T value } where tag=0 for None, 1 for Some.
    fn lower_maybe_builtin(
        &mut self,
        name: &str,
        args: Vec<MirValue>,
        data_type: &DataType,
        loc: (usize, usize),
    ) -> MirValue {
        let maybe_val = &args[0]; // receiver
        let inner_ty = match data_type {
            DataType::Maybe { inner } => *inner.clone(),
            _ => DataType::Unknown,
        };
        let _struct_ty = MirType {
            data_type: DataType::Maybe {
                inner: Box::new(inner_ty.clone()),
            },
        };

        match name {
            "maybe.is_some" => {
                // Extract tag (index 0) and compare with 1
                let tag = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(tag),
                    MirOp::ExtractValue(maybe_val.clone(), maybe_val.clone(), vec![0]),
                    loc,
                );
                let is_some = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(is_some),
                    MirOp::ICmp(
                        MirCmp::Eq,
                        MirValue::temp(tag),
                        MirValue::Const(MirConst::Int(1)),
                    ),
                    loc,
                );
                MirValue::temp(is_some)
            }
            "maybe.is_none" => {
                // Extract tag (index 0) and compare with 0
                let tag = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(tag),
                    MirOp::ExtractValue(maybe_val.clone(), maybe_val.clone(), vec![0]),
                    loc,
                );
                let is_none = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(is_none),
                    MirOp::ICmp(
                        MirCmp::Eq,
                        MirValue::temp(tag),
                        MirValue::Const(MirConst::Int(0)),
                    ),
                    loc,
                );
                MirValue::temp(is_none)
            }
            "maybe.unwrap.i64" | "maybe.unwrap.str" | "maybe.unwrap.f64" | "maybe.unwrap.ptr" => {
                // Extract value (index 1) - assume Some (caller checks with ?)
                let value = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(value),
                    MirOp::ExtractValue(maybe_val.clone(), maybe_val.clone(), vec![1]),
                    loc,
                );
                MirValue::temp(value)
            }
            "maybe.unwrap_or.i64"
            | "maybe.unwrap_or.str"
            | "maybe.unwrap_or.f64"
            | "maybe.unwrap_or.ptr" => {
                // args[0] = maybe, args[1] = default
                // Extract tag, if 1 return value, else return default
                let default = &args[1];
                let tag = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(tag),
                    MirOp::ExtractValue(maybe_val.clone(), maybe_val.clone(), vec![0]),
                    loc,
                );
                let is_some = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(is_some),
                    MirOp::ICmp(
                        MirCmp::Eq,
                        MirValue::temp(tag),
                        MirValue::Const(MirConst::Int(1)),
                    ),
                    loc,
                );
                // Extract value
                let value = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(value),
                    MirOp::ExtractValue(maybe_val.clone(), maybe_val.clone(), vec![1]),
                    loc,
                );
                // Use Select: is_some ? value : default
                let result = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(result),
                    MirOp::Select(
                        MirValue::temp(is_some),
                        MirValue::temp(value),
                        default.clone(),
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            _ => { debug_assert!(false, "unhandled Expression variant in lowering"); MirValue::Const(MirConst::None) },
        }
    }
}
