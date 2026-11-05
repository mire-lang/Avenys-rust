use super::MirLower;
use super::collections::lower_index_write;
use super::types::{
    extract_data_type, llvm_elem_type_str, llvm_type_byte_size, pointee_referenced_type,
};
use crate::compiler::location::statement_location;
use crate::compiler::mir::*;
use crate::parser::ast::{AssignmentTarget, DataType, Expression, Statement};

/// True when `emit_convert` can safely lower a value of `from` into `to`
/// (both are integer/float/char kinds). Avoids the `Sitofp` fallback that
/// would corrupt non-numeric values such as structs or pointers.
pub(crate) fn needs_convert(from: &DataType, to: &DataType) -> bool {
    use DataType::*;
    let numeric = |t: &DataType| {
        matches!(
            t,
            I8 | I16 | I32 | I64 | I128 | U8 | U16 | U32 | U64 | U128 | Char | F32 | F64
        )
    };
    from != to && *to != DataType::Unknown && numeric(from) && numeric(to)
}

/// Heap-owned pointer types whose slot assignment transfers ownership of the
/// pointer (the slot is responsible for releasing the old value). Arrays are
/// inline value types and references are borrowed, so neither is included.
pub(crate) fn is_owned_ptr_type(ty: &DataType) -> bool {
    matches!(
        ty,
        DataType::Str
            | DataType::Vector { .. }
            | DataType::Map { .. }
            | DataType::Dict
            | DataType::List
            | DataType::Slice { .. }
            | DataType::Set
            | DataType::Box
            | DataType::Result { .. }
    )
}

/// Classify ownership while the lowering phase still has the semantic type.
/// Codegen must not infer ownership from LLVM's undifferentiated `ptr` type.
///
/// INVARIANT: `Ownership` (the destructor) is a SUBSET of
/// `is_owned_ptr_type` (the property). Today the delta is exactly
/// `{Slice, Set, Box, Result}`: these are treated as owned and retained on
/// alias, but have no destructor here yet. When one gets a heap
/// representation, add an `Ownership` variant BEFORE wiring its free; never
/// merge these functions. In particular, `is_owned_ptr_type(Slice)` must stay
/// true while Slice's `Ownership` remains `None`.
pub(crate) fn ownership_of(ty: &DataType) -> Ownership {
    match ty {
        DataType::Str => Ownership::ManagedString,
        DataType::Vector { .. } | DataType::List => Ownership::List,
        DataType::Map { .. } | DataType::Dict => Ownership::Dict,
        DataType::Ref { inner } | DataType::RefMut { inner } => match inner.as_ref() {
            DataType::Str => Ownership::ManagedString,
            _ => Ownership::Borrowed,
        },
        _ => Ownership::None,
    }
}

/// True when evaluating `expr` yields a pointer *owned by something else* (a
/// variable or a struct field). Assigning a borrow into a slot must retain it,
/// otherwise releasing the source would leave the destination dangling.
fn is_borrow_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_) | Expression::MemberAccess { .. } => true,
        Expression::Ascription { expr, .. } => is_borrow_expression(expr),
        _ => false,
    }
}

/// The variable name a borrowed read ultimately aliases (for globals, which are
/// never released, no retain is needed).
fn borrow_source_name(expr: &Expression) -> Option<&str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::Ascription { expr, .. } => borrow_source_name(expr),
        _ => None,
    }
}

/// Collection mutators return the same owned collection they update. In
/// `set values = collection::set(values, key, item)`, the assignment must
/// transfer that storage into the same slot without dropping the old pointer
/// first. Otherwise the returned pointer may already have been freed.
fn collection_call_reuses_assignment_target(expr: &Expression, target: &str) -> bool {
    let Expression::Call { name, args, .. } = expr else {
        return false;
    };
    let is_collection_mutator = name == "lists.push"
        || name == "vec.push"
        || name.starts_with("vec.push.")
        || name == "map.set"
        || name.starts_with("map.set.")
        || name == "maps.set"
        || name.starts_with("maps.set.")
        || name == "dicts.set"
        || name.starts_with("dicts.set.");
    if !is_collection_mutator {
        return false;
    }
    matches!(args.first(), Some(Expression::Identifier(identifier)) if identifier.name == target)
}

impl MirLower {
    /// Emit `rt_managed_retain(value)` (a no-op for non-managed pointers).
    pub(crate) fn emit_retain(&mut self, value: MirValue, loc: (usize, usize)) {
        let last = self.current_block;
        self.func.blocks[last].push(
            None,
            MirOp::Call(
                MirValue::Global("rt_managed_retain".to_string()),
                vec![value],
                MirType {
                    data_type: DataType::None,
                },
            ),
            loc,
        );
    }
}

impl MirLower {
    pub(crate) fn lower_statement(&mut self, stmt: &Statement) {
        let loc = statement_location(stmt).to_tuple();
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
                // A declaration can execute repeatedly when it is inside a
                // loop. Initialize owned slots so the first cleanup is safe;
                // later iterations release the previous iteration's value
                // before storing the replacement.
                if is_owned_ptr_type(data_type) {
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        None,
                        MirOp::Store(MirValue::temp(ptr), MirValue::Const(MirConst::None)),
                        loc,
                    );
                }
                self.vars.insert(name.clone(), ptr);
                self.var_types.insert(name.clone(), data_type.clone());
                // Track this slot so scope exit can release it. The rebind path
                // below replaces the entry, since the previous value is released
                // by the rebind itself.
                self.track_owned_local(name, ptr, data_type.clone());

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
                    // `set b = a` aliases the source variable's value; retain it
                    // so the slot owns an independent reference.
                    if is_owned_ptr_type(data_type)
                        && is_borrow_expression(val)
                        && borrow_source_name(val)
                            .map(|n| !self.globals.contains_key(n))
                            .unwrap_or(true)
                    {
                        self.emit_retain(v.clone(), loc);
                    }
                    if is_owned_ptr_type(data_type) {
                        let loaded = self.new_temp();
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            Some(loaded),
                            MirOp::Load(
                                MirValue::temp(ptr),
                                MirType {
                                    data_type: data_type.clone(),
                                },
                            ),
                            loc,
                        );
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            None,
                            MirOp::Drop(MirValue::temp(loaded), ownership_of(data_type)),
                            loc,
                        );
                    }
                    let last = self.current_block;
                    self.func.blocks[last].push(None, MirOp::Store(MirValue::temp(ptr), v), loc);
                }
            }
            Statement::Assignment { target, value, .. } => {
                // Evaluate the right-hand side *before* releasing the old value:
                // the RHS may borrow the variable being reassigned (`set s = s +
                // "x"`), so dropping first would use freed memory.
                let val_ty = extract_data_type(value);
                let mut v = self.lower_expression(value);
                match target {
                    AssignmentTarget::Variable(name) => {
                        let target_ty = self
                            .var_types
                            .get(name)
                            .cloned()
                            .unwrap_or(DataType::Unknown);
                        if needs_convert(&val_ty, &target_ty) {
                            v = self.emit_convert(v, &val_ty, &target_ty, loc);
                        }
                        if self.globals.contains_key(name) {
                            let last = self.current_block;
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::Global(name.clone()), v),
                                loc,
                            );
                        } else if let Some(&ptr) = self.vars.get(name) {
                            // `set y = x` aliases another slot's value: retain it
                            // so releasing the old y (and possibly x) is safe.
                            if is_owned_ptr_type(&target_ty)
                                && is_borrow_expression(value)
                                && borrow_source_name(value)
                                    .map(|n| !self.globals.contains_key(n))
                                    .unwrap_or(true)
                            {
                                self.emit_retain(v.clone(), loc);
                            }
                            // Release the value the slot currently holds (only
                            // for owned pointer types).
                            let result_reuses_target =
                                collection_call_reuses_assignment_target(value, name);
                            if is_owned_ptr_type(&target_ty) && !result_reuses_target {
                                let loaded = self.new_temp();
                                let last = self.current_block;
                                self.func.blocks[last].push(
                                    Some(loaded),
                                    MirOp::Load(
                                        MirValue::temp(ptr),
                                        MirType {
                                            data_type: target_ty.clone(),
                                        },
                                    ),
                                    loc,
                                );
                                let last = self.current_block;
                                self.func.blocks[last].push(
                                    None,
                                    MirOp::Drop(MirValue::temp(loaded), ownership_of(&target_ty)),
                                    loc,
                                );
                            }
                            let last = self.current_block;
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::temp(ptr), v),
                                loc,
                            );
                            // The rebind above released the previous value, so
                            // this slot now owns the new one and scope exit must
                            // release it.
                            //
                            // Track the slot unconditionally, including for a
                            // collection mutator (`vec::push(v, x)`,
                            // `map::set(m, k, v)`). Such a call returns the SAME
                            // storage with the element added, so the rebind
                            // deliberately did not release the old value — but
                            // the slot still owns that allocation, and scope exit
                            // is the only place left that will release it.
                            //
                            // Not tracking it here was the leak: the binding
                            // tracked, the first `push` released nothing and
                            // cleared the entry, and every later iteration then
                            // had no tracked local at all.
                            self.track_owned_local(name, ptr, target_ty.clone());
                        }
                    }
                    AssignmentTarget::Index { target, index } => {
                        let target_type = if let Expression::Identifier(id) = target.as_ref() {
                            self.var_types
                                .get(&id.name)
                                .cloned()
                                .unwrap_or(DataType::Unknown)
                        } else {
                            extract_data_type(target)
                        };
                        // Indexed stores auto-deref references, matching reads.
                        let target_type = match target_type {
                            DataType::Ref { inner } | DataType::RefMut { inner } => *inner,
                            other => other,
                        };
                        let value_type = extract_data_type(value);
                        if lower_index_write(self, target, index, v.clone(), &value_type) {
                            return;
                        }

                        let target_val = self.lower_expression(target);
                        let index_val = self.lower_expression(index);
                        let mut merge_block: Option<usize> = None;

                        match &target_type {
                            DataType::Array { size, .. } => {
                                let last = self.current_block;
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
                                let len_val = self.emit_collection_len(target_val.clone(), loc);
                                merge_block =
                                    Some(self.emit_bounds_check(index_val.clone(), len_val, loc));
                            }
                            _ => {}
                        }

                        let last = self.current_block;
                        let gep = self.new_temp();
                        let elem_ty = self.get_target_elem_type(target);
                        let adjusted_index =
                            if matches!(target_type, DataType::Vector { .. } | DataType::List) {
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
                        if let Some(cont) = merge_block {
                            let last = self.current_block;
                            self.func.blocks[last].terminator = MirTerminator::Br(cont);
                            self.current_block = cont;
                        }
                    }
                    AssignmentTarget::Dereference { target } => {
                        // `*p = v` is a direct store. The pointer expression
                        // yields the address (an extern declared `:&u32` lowers
                        // to one), so the value is stored straight through it
                        // with no runtime call.
                        let ptr_val = self.lower_expression(target);
                        let pointee = extract_data_type(target);
                        let mut stored = v;
                        let store_ty = pointee_referenced_type(&pointee).unwrap_or(pointee.clone());
                        let val_ty = extract_data_type(value);
                        if needs_convert(&val_ty, &store_ty) {
                            stored = self.emit_convert(stored, &val_ty, &store_ty, loc);
                        }
                        let last = self.current_block;
                        self.func.blocks[last].push(None, MirOp::Store(ptr_val, stored), loc);
                    }
                    AssignmentTarget::Field(path) => {
                        let last = self.current_block;
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
                // `do { body } while cond` is parsed as a call to a synthetic
                // `__do_while` name with two closures: (body statements) and
                // (cond as a `return` expression). Lower it to real control
                // flow instead of emitting an undefined external call.
                if let Expression::Call { name, args, .. } = expr
                    && name == "__do_while"
                    && args.len() == 2
                {
                    let body_stmts: Option<&Vec<Statement>> = match &args[0] {
                        Expression::Closure { body, .. } => Some(body),
                        _ => None,
                    };
                    let cond_expr: Option<&Expression> = match &args[1] {
                        Expression::Closure { body, .. } => match body.first() {
                            Some(Statement::Return(Some(inner))) => Some(inner),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let (Some(body_stmts), Some(cond_expr)) = (body_stmts, cond_expr) {
                        let pre_do_block = self.current_block;

                        let body_block = self.new_block("dowhile_body");
                        let cond_block = self.new_block("dowhile_cond");
                        let end_block = self.new_block("dowhile_end");

                        self.loop_stack.push((cond_block, end_block));
                        self.current_block = body_block;

                        for stmt in body_stmts {
                            self.lower_statement(stmt);
                        }
                        self.loop_stack.pop();
                        if matches!(
                            self.func.blocks[self.current_block].terminator,
                            MirTerminator::Unreachable
                        ) {
                            self.func.blocks[self.current_block].terminator =
                                MirTerminator::Br(cond_block);
                        }

                        self.current_block = cond_block;
                        let cond = self.lower_expression(cond_expr);
                        let cond_branch_block = self.current_block;
                        self.func.blocks[cond_branch_block].terminator =
                            MirTerminator::BrCond(cond, body_block, end_block);
                        self.func.blocks[pre_do_block].terminator = MirTerminator::Br(body_block);
                        self.current_block = end_block;
                        return;
                    }
                }
                let _ = self.lower_expression(expr);
            }
            Statement::Return(val) => {
                let ret_ty = self.func.ret_type.clone();
                let loc = statement_location(&Statement::Return(val.clone())).to_tuple();
                // Convert on the way out, exactly as a `let` with an ascription
                // does. The checker lets an integer widen through a return --
                // `return small()` from a `:i64` function where `small` returns
                // `:u8` -- and every other implicit conversion is materialised
                // during lowering. Returns were the one that was not: the value
                // reached the terminator at its own width and the backend emitted
                // `ret i8` out of an `i64` function, which llc rejected with no
                // source location and nothing pointing at the return.
                //
                // This has to be emit_convert and not a widening in the backend:
                // the backend only sees LLVM type names, which carry no
                // signedness, so it cannot tell 200u8 (which must widen to 200)
                // from a negative i8 (which must widen to -56).
                let v = val.as_ref().map(|e| {
                    let src_ty = extract_data_type(e);
                    let lowered = self.lower_expression(e);
                    if needs_convert(&src_ty, &ret_ty) {
                        self.emit_convert(lowered, &src_ty, &ret_ty, loc)
                    } else {
                        lowered
                    }
                });
                let v = v.map(|x| self.materialize_array_value(x, &ret_ty, loc));

                // Anything the return expression reaches has escaped to the
                // caller and must not be released here.
                if let Some(expr) = val.as_ref() {
                    self.mark_escaped_in_expression(expr);
                }

                // Release locals that are still owned here. Without this a
                // function that built a vector and returned a number leaked the
                // vector on every call. A local that *is* the returned value, or
                // that the return expression reaches through, was recorded as
                // escaped above and is skipped — releasing it would free what
                // the caller now owns.
                //
                // Every open frame is released, not just the innermost one. At
                // the top level of a body the innermost frame is the function's
                // own; releasing and popping just that one made the next
                // top-level `return` operate on a scope that no longer existed,
                // which released an already-released local a second time.
                self.release_all_frames();

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
                // The condition expression may have created intermediate blocks
                // (e.g. division inline). Use the final block as the condition branch source.
                let cond_branch_block = self.current_block;

                let body_block = self.new_block("while_body");
                let end_block = self.new_block("while_end");

                self.loop_stack.push((cond_block, end_block));
                self.current_block = body_block;

                // The body is its own scope. A value declared inside it is
                // released when the iteration ends, which is the only place
                // that can happen for a loop building one value per iteration.
                self.begin_scope();
                for stmt in body {
                    self.lower_statement(stmt);
                }
                self.loop_stack.pop();
                // Release this iteration's locals before branching back to the
                // condition, then pop the frame so the next iteration starts
                // from a clean scope while enclosing scopes keep theirs.
                self.end_scope_and_release();
                if matches!(
                    self.func.blocks[self.current_block].terminator,
                    MirTerminator::Unreachable
                ) {
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(cond_block);
                }

                self.func.blocks[cond_branch_block].terminator =
                    MirTerminator::BrCond(cond, body_block, end_block);
                self.func.blocks[pre_while_block].terminator = MirTerminator::Br(cond_block);
                self.current_block = end_block;
            }
            // `find` is the same iteration as `for` -- bind the element, run
            // the body, let `return` inside it exit early -- with no index to
            // bind. It had no arm of its own and fell through to `_ => {}`, so
            // the body was never lowered and the loop compiled to nothing at
            // all: `find x in [1 2 3] { dasu(x) }` built and linked cleanly and
            // printed nothing.
            Statement::For {
                variable,
                index: _,
                iterable,
                body,
            }
            | Statement::Find {
                variable,
                iterable,
                body,
                ..
            } => {
                // `for` carries an optional index binding; `find` does not.
                let index = match stmt {
                    Statement::For { index, .. } => index.clone(),
                    _ => None,
                };
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
                let inc_block = self.new_block("for_inc");
                let end_block = self.new_block("for_end");

                self.loop_stack.push((inc_block, end_block));
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
                self.loop_stack.pop();

                let last_body = self.current_block;
                let body_terminated = !matches!(
                    self.func.blocks[last_body].terminator,
                    MirTerminator::Unreachable
                );
                if !body_terminated {
                    self.func.blocks[last_body].terminator = MirTerminator::Br(inc_block);
                }

                self.current_block = inc_block;
                let old_idx = self.new_temp();
                self.func.blocks[inc_block].push(
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
                self.func.blocks[inc_block].push(
                    Some(new_idx),
                    MirOp::Add(MirValue::temp(old_idx), MirValue::Const(MirConst::Int(1))),
                    loc,
                );
                self.func.blocks[inc_block].push(
                    None,
                    MirOp::Store(MirValue::temp(idx_ptr), MirValue::temp(new_idx)),
                    loc,
                );
                self.func.blocks[inc_block].terminator = MirTerminator::Br(cond_block);

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
            Statement::Match {
                value,
                cases,
                default,
            } => {
                let match_val = self.lower_expression(value);
                let n = cases.len();

                // Allocate every block up front so that case bodies (which may
                // create their own blocks via if/while/for/nested match) never
                // shift the fixed block indices the chk chain points at.
                let mut chk_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    chk_blocks.push(self.new_block(&format!("stmt_match_chk_{}", i)));
                }
                let mut case_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    case_blocks.push(self.new_block(&format!("stmt_match_case_{}", i)));
                }
                let default_idx = self.new_block("stmt_match_default");
                let end_idx = self.new_block("stmt_match_end");

                self.func.blocks[self.current_block].terminator =
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

                for (i, (pattern, case_body)) in cases.iter().enumerate() {
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
                    for s in case_body {
                        self.lower_statement(s);
                    }
                    if matches!(
                        self.func.blocks[self.current_block].terminator,
                        MirTerminator::Unreachable
                    ) {
                        self.func.blocks[self.current_block].terminator =
                            MirTerminator::Br(end_idx);
                    }
                }

                self.current_block = default_idx;
                for s in default {
                    self.lower_statement(s);
                }
                if matches!(
                    self.func.blocks[self.current_block].terminator,
                    MirTerminator::Unreachable
                ) {
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                self.current_block = end_idx;
            }
            Statement::Break => {
                // Everything after this is unreachable on the break path, so the
                // enclosing scope must not release what follows.
                self.block_scope_exit();
                if let Some(&(_, break_target)) = self.loop_stack.last() {
                    let last = self.current_block;
                    self.func.blocks[last].terminator = MirTerminator::Br(break_target);
                    // Route any following (unreachable) statements into a fresh
                    // block so they don't get appended to a terminated block.
                    self.current_block = self.new_block("after_break");
                }
            }
            Statement::Continue => {
                self.block_scope_exit();
                if let Some(&(continue_target, _)) = self.loop_stack.last() {
                    let last = self.current_block;
                    self.func.blocks[last].terminator = MirTerminator::Br(continue_target);
                    self.current_block = self.new_block("after_continue");
                }
            }
            // `move::(src) to dst` was silently dropped here: it reached the
            // catch-all, so no slot was ever created for `dst` and the borrow
            // checker's binding referred to a name codegen had never defined.
            // The symptom was an llc error, `use of undefined value '@dst'`,
            // pointing at a use *after* a correct move.
            //
            // It lowers like `set dst = src`: the value is shallowly
            // transferred. No destructor is emitted for the source, because
            // there are none anywhere in the lowering -- the borrow checker is
            // what makes the source unusable, so nulling it here would be
            // ceremony for a state the compiler has already proven unreachable.
            Statement::Move { target, value } => {
                let data_type = self
                    .var_types
                    .get(value_name(value))
                    .cloned()
                    .or_else(|| self.var_types.get(target.as_str()).cloned())
                    .unwrap_or(DataType::Unknown);

                if self.globals.contains_key(target) {
                    if self.func.blocks.is_empty() {
                        self.func.push_block("entry".to_string());
                    }
                    let v = self.lower_expression(value);
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        None,
                        MirOp::Store(MirValue::Global(target.clone()), v),
                        loc,
                    );
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
                self.vars.insert(target.clone(), ptr);
                self.var_types.insert(target.clone(), data_type.clone());

                let mut v = self.lower_expression(value);
                let val_ty = extract_data_type(value);
                if needs_convert(&val_ty, &data_type) {
                    v = self.emit_convert(v, &val_ty, &data_type, loc);
                }
                let last = self.current_block;
                self.func.blocks[last].push(None, MirOp::Store(MirValue::temp(ptr), v), loc);
            }
            _ => {}
        }
    }
}

/// The name a move source refers to, when it is a plain identifier.
///
/// `move::(a) to b` moves a binding, so the type has to come from `a`'s slot.
/// A computed source has no name to look up, and the caller's `unwrap_or`
/// covers that.
fn value_name(value: &Expression) -> &str {
    match value {
        Expression::Identifier(id) => id.name.as_str(),
        _ => "",
    }
}
