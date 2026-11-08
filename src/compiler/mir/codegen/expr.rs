use super::builtins::{builtin_to_pal, compile_pal_builtin};
use super::resolve::{coerce_to, coerce_to_bool, resolve_named_call, resolve_typed};
use super::types::{llvm_type_str, render_struct_llvm_type};
use super::{LlvmCtx, VEC_GROW_PUSH_I64, sanitize_fn_name, tmp_extra, tmp_result};
use crate::compiler::mir::{DataType, MirCmp, MirConst, MirInst, MirOp, MirValue, Ownership};

/// Resolves the common floating-point result type for a binary operation.
/// Returns `Some("float")` / `Some("double")` if either operand is
/// floating-point, or `None` if both are integers (use "i64" by default).
fn float_result_ty(lt: &str, rt: &str) -> Option<&'static str> {
    let lf = lt == "float" || lt == "double";
    let rf = rt == "float" || rt == "double";
    if lf || rf {
        if lt == "double" || rt == "double" {
            Some("double")
        } else {
            Some("float")
        }
    } else {
        None
    }
}

pub(crate) enum InstCompile {
    /// Normal instruction: lines of LLVM IR, block continues.
    Normal(Vec<String>),
    /// Instruction splits the current block with a conditional branch.
    /// `before` = lines emitted before the branch in the current block.
    /// `term` = the `br` terminator for the current block.
    /// `successors` = list of (label, lines, terminator) for successor blocks,
    /// emitted immediately after the current block. A terminator of `None` means
    /// the successor block falls through to following instructions.
    Split {
        before: Vec<String>,
        term: String,
        successors: Vec<(String, Vec<String>, Option<String>)>,
    },
}

/// The statically known name of a call target, if it has one.
///
/// Both callee spellings have to be accepted: a `Global` is a direct reference
/// to a runtime entry point, while a `FunctionRef` carries the same name plus
/// the environment pointer a Mire function needs. Matching on one variant alone
/// drops every call that arrived through the other.
fn call_target_name(callee: &MirValue) -> Option<&str> {
    match callee {
        MirValue::Global(name) => Some(name.as_str()),
        MirValue::FunctionRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

pub(crate) fn compile_inst(inst: &MirInst, ctx: &mut LlvmCtx) -> InstCompile {
    // ── Inline expansion of vec runtime calls ───────────────────────────
    // Intercept calls to rt_vecs_* and emit LLVM inline code instead of an
    // extern call, avoiding the ABI overhead on the hottest paths.
    //
    // The callee has to be matched through `call_target_name`, not through a
    // `MirValue::Global` pattern. A call that reaches codegen through an
    // imported module -- which is what every `vec::push` in a program that
    // does `load mire::vec` is -- carries a `MirValue::FunctionRef` instead,
    // and matching only `Global` silently skipped every one of them: the
    // lowering below existed, ran zero times, and each push stayed an
    // out-of-line call into a runtime translation unit with no LTO.
    if let MirOp::Call(callee, args, ret_ty) = &inst.op {
        let name = call_target_name(callee);
        let name = match name {
            Some(n) => n,
            None => "",
        };
        if (name == "rt_vecs_push_i64" || name == "vec.push.i64") && args.len() == 2 {
            return compile_vec_push_i64(inst, ctx, args);
        }
        if name == "rt_vecs_len" && args.len() == 1 {
            return compile_vec_len(inst, ctx, args);
        }
        if name == "rt_vecs_get_i64" && args.len() == 2 {
            return compile_vec_get_i64(inst, ctx, args);
        }
        if name == "rt_vecs_with_capacity_i64" && args.len() == 1 {
            return compile_vec_with_capacity_i64(inst, ctx, args);
        }
        if name == "rt_list_free" && args.len() == 1 {
            return compile_vec_free(ctx, args);
        }
        let _ = ret_ty;
    }

    // A dropped vector is the same null-guarded `free` of the header word that
    // an explicit `rt_list_free` call lowers to. Take that path here too: the
    // drop used to emit a plain `call @rt_list_free`, which is a tcc-compiled
    // function and stays an out-of-line call even though it only forwards to
    // `free`. A loop that allocates and drops a vector per iteration
    // (alloc_pressure does it eight million times) then pays that call on
    // every pass for nothing.
    if let MirOp::Drop(val, Ownership::List) = &inst.op {
        return compile_vec_free(ctx, std::slice::from_ref(val));
    }

    let mut extra = Vec::new();
    let line = match &inst.op {
        MirOp::Alloca(ty) => {
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, "ptr", inst.result);
            extra.push(format!("%t{} = alloca {}", result, llty));
            if llty == "ptr" {
                // Zero-initialize string pointer vars so first Store doesn't free garbage
                extra.push(format!("store ptr null, ptr %t{}", result));
            } else if llty == "{ ptr, ptr }" {
                extra.push(format!("store {llty} zeroinitializer, ptr %t{result}"));
            }
            String::new()
        }
        MirOp::Load(src, ty) => {
            let (src_s, _) = resolve_typed(src, ctx);
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &llty, inst.result);
            format!("%t{} = load {}, ptr {}", result, llty, src_s)
        }
        MirOp::Store(dst, src) => match src {
            MirValue::FunctionRef { name, env } => {
                let (dst_s, _) = resolve_typed(dst, ctx);
                let fn_name = format!("@fn_{}", sanitize_fn_name(name));
                let (env_str, env_ty) = resolve_typed(env, ctx);
                let fn_gep = tmp_extra(ctx, "ptr");
                let env_gep = tmp_extra(ctx, "ptr");
                extra.push(format!(
                    "{fn_gep} = getelementptr {{ ptr, ptr }}, ptr {dst_s}, i32 0, i32 0"
                ));
                extra.push(format!(
                    "{env_gep} = getelementptr {{ ptr, ptr }}, ptr {dst_s}, i32 0, i32 1"
                ));
                extra.push(format!("store ptr {fn_name}, ptr {fn_gep}"));
                let env_casted = if env_ty != "ptr" {
                    let tmp = tmp_extra(ctx, "ptr");
                    extra.push(format!("{tmp} = inttoptr {env_ty} {env_str} to ptr"));
                    tmp
                } else {
                    env_str
                };
                extra.push(format!("store ptr {env_casted}, ptr {env_gep}"));
                String::new()
            }
            // `none` defaults to i64 when nothing else says otherwise, which
            // is right for an integer slot and wrong for a pointer one:
            // `store i64 0, ptr %slot` is type-confused IR that LLVM has to
            // keep reloading through. Owned locals are initialised with `none`
            // and released later, so this is the shape that decides whether
            // the optimizer can see the slot as null.
            MirValue::Const(MirConst::None) => {
                let (dst_s, _) = resolve_typed(dst, ctx);
                format!("store ptr null, ptr {}", dst_s)
            }
            _ => {
                let (src_s, src_ty) = resolve_typed(src, ctx);
                let (dst_s, _) = resolve_typed(dst, ctx);
                format!("store {} {}, ptr {}", src_ty, src_s, dst_s)
            }
        },
        MirOp::Add(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            if lt == "ptr" || rt == "ptr" {
                // String concatenation: emit call to rt_string_concat
                let result = tmp_result(ctx, "ptr", inst.result);
                let lhs = if lt != "ptr" {
                    let conv = tmp_extra(ctx, "i64");
                    extra.push(format!("{} = inttoptr i64 {} to ptr", conv, l_str));
                    conv
                } else {
                    l_str.clone()
                };
                let rhs = if rt != "ptr" {
                    let conv = tmp_extra(ctx, "i64");
                    extra.push(format!("{} = inttoptr i64 {} to ptr", conv, r_str));
                    conv
                } else {
                    r_str.clone()
                };
                // Emit the concat call
                let concat_call = format!(
                    "%t{} = call ptr @rt_string_concat(ptr {}, ptr {})",
                    result, lhs, rhs
                );
                // A left operand that is a temporary is an intermediate in a
                // concat chain, and `rt_string_concat` copies the bytes, so
                // the allocation can be released after use.
                //
                // The condition must match `MirOp::Concat`'s lowering, which
                // decides with `concat_operand_is_owned`: a fresh allocation
                // whose result is a pointer. Testing the LLVM type name here
                // instead would free a borrowed variable that happens to be
                // held in a `ptr`-typed slot — LLVM's `ptr` does not
                // distinguish a string from a vector, a map, or a borrow, so
                // the type name cannot answer the ownership question.
                let free_lhs = if lt == "ptr" && l_str.starts_with('%') {
                    format!("\n  call void @rt_managed_free(ptr {})", lhs)
                } else {
                    String::new()
                };
                format!("{}{}", concat_call, free_lhs)
            } else {
                let ty = float_result_ty(&lt, &rt).unwrap_or("i64");
                let result = tmp_result(ctx, ty, inst.result);
                let op = match ty {
                    "double" => "fadd",
                    "float" => "fadd",
                    _ => "add",
                };
                let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
                let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
                format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
            }
        }
        MirOp::Sub(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = float_result_ty(&lt, &rt).unwrap_or("i64");
            let result = tmp_result(ctx, ty, inst.result);
            let op = match ty {
                "double" => "fsub",
                "float" => "fsub",
                _ => "sub",
            };
            let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
            format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
        }
        MirOp::Mul(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = float_result_ty(&lt, &rt).unwrap_or("i64");
            let result = tmp_result(ctx, ty, inst.result);
            let op = match ty {
                "double" => "fmul",
                "float" => "fmul",
                _ => "mul",
            };
            let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
            format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
        }
        MirOp::SDiv(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let is_float = float_result_ty(&lt, &rt).is_some();
            let ty = if is_float { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
            if is_float {
                format!("%t{} = fdiv double {}, {}", result, l_final, r_final)
            } else {
                format!("%t{} = sdiv i64 {}, {}", result, l_final, r_final)
            }
        }
        MirOp::SRem(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let is_float = float_result_ty(&lt, &rt).is_some();
            let ty = if is_float { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
            if is_float {
                format!("%t{} = frem double {}, {}", result, l_final, r_final)
            } else {
                format!("%t{} = srem i64 {}, {}", result, l_final, r_final)
            }
        }
        MirOp::Concat(vals) => {
            let result = tmp_result(ctx, "ptr", inst.result);
            // Build array of pointers for rt_string_concat_n
            let array_ptr = tmp_extra(ctx, "ptr");
            let array_type = format!("[{} x ptr]", vals.len());
            extra.push(format!("{} = alloca {}, align 8", array_ptr, array_type));
            for (i, val) in vals.iter().enumerate() {
                let (v_str, v_ty) = resolve_typed(val, ctx);
                let elem_ptr = tmp_extra(ctx, "ptr");
                extra.push(format!(
                    "{} = getelementptr inbounds {}, ptr {}, i32 0, i64 {}",
                    elem_ptr, array_type, array_ptr, i
                ));
                if v_ty != "ptr" {
                    let conv = tmp_extra(ctx, "i64");
                    extra.push(format!("{} = inttoptr i64 {} to ptr", conv, v_str));
                    extra.push(format!("store ptr {}, ptr {}", conv, elem_ptr));
                } else {
                    extra.push(format!("store ptr {}, ptr {}", v_str, elem_ptr));
                }
            }
            let count = vals.len();
            format!(
                "%t{} = call ptr @rt_string_concat_n(i64 {}, ptr {})",
                result, count, array_ptr
            )
        }
        MirOp::Shl(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            format!("%t{} = shl i64 {}, {}", result, l, r)
        }
        MirOp::Shr(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            // ashr, not lshr: `>>` on a negative value has to sign-extend, or
            // -1 >> 1 yields 9223372036854775807 instead of -1. Every other
            // language with a signed i64 shifts arithmetically, so lshr here was
            // simply wrong, and wrong silently.
            format!("%t{} = ashr i64 {}, {}", result, l, r)
        }
        MirOp::Xor(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            format!("%t{} = xor i64 {}, {}", result, l, r)
        }
        MirOp::BitAnd(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            format!("%t{} = and i64 {}, {}", result, l, r)
        }
        MirOp::BitOr(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            format!("%t{} = or i64 {}, {}", result, l, r)
        }
        MirOp::And(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "i1" || rt == "i1" {
                "i1"
            } else {
                "i64"
            };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to_bool(&l_str, &lt, ctx, &mut extra);
            let r_final = coerce_to_bool(&r_str, &rt, ctx, &mut extra);
            format!("%t{} = and {} {}, {}", result, ty, l_final, r_final)
        }
        MirOp::Or(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "i1" || rt == "i1" {
                "i1"
            } else {
                "i64"
            };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to_bool(&l_str, &lt, ctx, &mut extra);
            let r_final = coerce_to_bool(&r_str, &rt, ctx, &mut extra);
            format!("%t{} = or {} {}, {}", result, ty, l_final, r_final)
        }
        MirOp::ICmp(cmp, l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i1", inst.result);
            if lt == "double" || rt == "double" {
                let ty = "double";
                let cond = match cmp {
                    MirCmp::Eq => "oeq",
                    MirCmp::Ne => "une",
                    MirCmp::Lt => "olt",
                    MirCmp::Le => "ole",
                    MirCmp::Gt => "ogt",
                    MirCmp::Ge => "oge",
                };
                let l_final = coerce_to(&l_str, &lt, ty, ctx, &mut extra);
                let r_final = coerce_to(&r_str, &rt, ty, ctx, &mut extra);
                format!(
                    "%t{} = fcmp {} {} {}, {}",
                    result, cond, ty, l_final, r_final
                )
            } else if lt == "ptr" || rt == "ptr" {
                let cond = match cmp {
                    MirCmp::Eq => "eq",
                    MirCmp::Ne => "ne",
                    _ => "eq",
                };
                if lt == "ptr" && rt == "ptr" {
                    // String content comparisons have already been lowered
                    // explicitly from their semantic types. Two remaining
                    // pointer operands are raw identities and must never be
                    // passed to strcmp (they may not be NUL-terminated).
                    format!("%t{} = icmp {} ptr {}, {}", result, cond, l_str, r_str)
                } else {
                    // Pointer against an integer: an address comparison. This is
                    // the null check, `if ptr == 0`, and it used to be lowered to
                    // strcmp with an inttoptr'd literal, which dereferenced the
                    // null pointer and segfaulted. Any `ptr` used with a raw
                    // allocation (rt_alloc_raw) needs this to be a plain icmp.
                    let l_ptr = if lt != "ptr" {
                        let conv = tmp_extra(ctx, "ptr");
                        extra.push(format!("{} = inttoptr i64 {} to ptr", conv, l_str));
                        conv
                    } else {
                        l_str.clone()
                    };
                    let r_ptr = if rt != "ptr" {
                        let conv = tmp_extra(ctx, "ptr");
                        extra.push(format!("{} = inttoptr i64 {} to ptr", conv, r_str));
                        conv
                    } else {
                        r_str.clone()
                    };
                    format!("%t{} = icmp {} ptr {}, {}", result, cond, l_ptr, r_ptr)
                }
            } else {
                let cond = match cmp {
                    MirCmp::Eq => "eq",
                    MirCmp::Ne => "ne",
                    MirCmp::Lt => "slt",
                    MirCmp::Le => "sle",
                    MirCmp::Gt => "sgt",
                    MirCmp::Ge => "sge",
                };
                format!("%t{} = icmp {} {} {}, {}", result, cond, lt, l_str, r_str)
            }
        }
        MirOp::FCmp(cmp, l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i1", inst.result);
            let cond = match cmp {
                MirCmp::Eq => "oeq",
                MirCmp::Ne => "one",
                MirCmp::Lt => "olt",
                MirCmp::Le => "ole",
                MirCmp::Gt => "ogt",
                MirCmp::Ge => "oge",
            };
            format!("%t{} = fcmp {} double {}, {}", result, cond, l, r)
        }
        MirOp::Call(callee, args, ret_ty) => {
            let name_opt: Option<&str> = match callee {
                MirValue::FunctionRef { name, .. } | MirValue::Global(name) => Some(name.as_str()),
                _ => None,
            };
            if name_opt == Some("str") {
                // Dedicated str() builtin lowering
                if args.len() != 1 {
                    return InstCompile::Normal(vec![]);
                }
                let (v, t) = resolve_typed(&args[0], ctx);
                let result = tmp_result(ctx, "ptr", inst.result);

                match t.as_str() {
                    "ptr" => {
                        // Identity — already a string ptr
                        format!("%t{} = select i1 true, ptr {}, ptr {}", result, v, v)
                    }
                    "i1" => {
                        let zext = tmp_extra(ctx, "i64");
                        extra.push(format!("{} = zext i1 {} to i64", zext, v));
                        format!("%t{} = call ptr @rt_bool_to_string(i64 {})", result, zext)
                    }
                    "double" => {
                        format!("%t{} = call ptr @rt_f64_to_string(double {})", result, v)
                    }
                    "float" => {
                        format!("%t{} = call ptr @rt_f32_to_string(float {})", result, v)
                    }
                    "i128" | "u128" => {
                        let rt = if t == "u128" {
                            "rt_u128_to_string"
                        } else {
                            "rt_i128_to_string"
                        };
                        format!("%t{} = call ptr @{}(i128 {})", result, rt, v)
                    }
                    _ => {
                        // All integer widths coerce losslessly up to i64.
                        let cv = coerce_to(&v, &t, "i64", ctx, &mut extra);
                        format!("%t{} = call ptr @rt_i64_to_string(i64 {})", result, cv)
                    }
                }
            } else if name_opt == Some("dasu") || name_opt == Some("print") {
                // dasu() / print() builtin expansion
                if args.is_empty() {
                    return InstCompile::Normal(vec![]);
                }
                let (v, t) = resolve_typed(&args[0], ctx);
                let printf_tmp = tmp_extra(ctx, "i32");
                let line = match t.as_str() {
                    "i1" => {
                        let select = tmp_extra(ctx, "ptr");
                        let label = tmp_extra(ctx, "i1");
                        extra.push(format!("{} = icmp eq i1 {}, 1", label, v));
                        extra.push(format!(
                            "{} = select i1 {}, ptr @.fmt_bool_true, ptr @.fmt_bool_false",
                            select, label
                        ));
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                            printf_tmp, select
                        )
                    }
                    "ptr" => {
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                            printf_tmp, v
                        )
                    }
                    "double" => {
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_f64, double {})",
                            printf_tmp, v
                        )
                    }
                    "float" => {
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_f64, double {})",
                            printf_tmp,
                            coerce_to(&v, &t, "double", ctx, &mut extra)
                        )
                    }
                    "i128" | "u128" => {
                        let s_tmp = tmp_extra(ctx, "ptr");
                        let rt = if t == "u128" {
                            "rt_u128_to_string"
                        } else {
                            "rt_i128_to_string"
                        };
                        extra.push(format!("{} = call ptr @{}(i128 {})", s_tmp, rt, v));
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                            printf_tmp, s_tmp
                        )
                    }
                    _ => {
                        // All integer widths coerce losslessly up to i64 for %lld.
                        let cv = coerce_to(&v, &t, "i64", ctx, &mut extra);
                        format!(
                            "{} = call i32 (ptr, ...) @printf(ptr @.fmt_i64, i64 {})",
                            printf_tmp, cv
                        )
                    }
                };
                let result = tmp_result(ctx, "ptr", inst.result);
                // Order matters here, and it is the opposite of what the `extra`
                // convention suggests. compile_inst emits everything in `extra`
                // *before* the line this arm returns, because `extra` normally
                // holds setup (allocas, coercions) that the operation depends on.
                // A flush is not setup, it is the last step: flushing first would
                // push the *previous* line out and leave this one sitting in the
                // buffer. So the printf, the dummy result and the flush all go
                // into `extra`, in that order, and the returned line is empty.
                //
                // The bug this fixes is only visible when stdout is not a
                // terminal. dasu() writing into a fully buffered stdout while
                // stderr stays unbuffered meant a program alternating the two
                // had its stdout output arrive after the stderr that followed it,
                // and stdout was not pushed out at all until exit.
                extra.push(line);
                extra.push(format!("%t{} = inttoptr i64 0 to ptr", result));
                extra.push("call i32 @fflush(ptr null)".to_string());
                // result is the ptr returned by dasu (dummy null pointer)
                String::new()
            } else if name_opt == Some("ireru") {
                let prompt_ptr = if args.is_empty() {
                    "null".to_string()
                } else {
                    let (v, _t) = resolve_typed(&args[0], ctx);
                    v
                };
                let result = tmp_result(ctx, "ptr", inst.result);
                // Same ordering fix as dasu: write the prompt first, then flush,
                // otherwise the prompt is still sitting in the buffer when the
                // flush runs and the reader sees nothing.
                extra.push(format!(
                    "%t{} = call ptr @ireru(ptr {})",
                    result, prompt_ptr
                ));
                extra.push("call i32 @fflush(ptr null)".to_string());
                String::new()
            } else if name_opt == Some("env_args") {
                // env_args() builtin — delegate to runtime
                let result = tmp_result(ctx, "ptr", inst.result);
                let argc = tmp_extra(ctx, "i32");
                let argv = tmp_extra(ctx, "ptr");
                extra.push(format!("{} = load i32, ptr @.argc", argc));
                extra.push(format!("{} = load ptr, ptr @.argv", argv));
                format!(
                    "%t{} = call ptr @rt_get_args(i32 {}, ptr {})",
                    result, argc, argv
                )
            } else if name_opt == Some("thread.spawn") {
                if args.is_empty() {
                    String::new()
                } else {
                    let result = tmp_result(ctx, "i64", inst.result);
                    match &args[0] {
                        MirValue::FunctionRef { name, env } => {
                            let fn_name = format!("@fn_{}", sanitize_fn_name(name));
                            let (env_str, env_ty) = resolve_typed(env, ctx);
                            let env_cast = if env_ty != "ptr" {
                                let tmp = tmp_extra(ctx, "ptr");
                                extra.push(format!(
                                    "{} = inttoptr {} {} to ptr",
                                    tmp, env_ty, env_str
                                ));
                                tmp
                            } else {
                                env_str
                            };
                            format!(
                                "%t{} = call i64 @rt_thread_spawn_closure(ptr {}, ptr {})",
                                result, fn_name, env_cast
                            )
                        }
                        MirValue::Temp(_id) => {
                            let (val, ty) = resolve_typed(&args[0], ctx);
                            if ty == "{ ptr, ptr }" {
                                let fn_ptr = tmp_extra(ctx, "ptr");
                                let env_ptr = tmp_extra(ctx, "ptr");
                                extra.push(format!(
                                    "{fn_ptr} = extractvalue {{ ptr, ptr }} {val}, 0"
                                ));
                                extra.push(format!(
                                    "{env_ptr} = extractvalue {{ ptr, ptr }} {val}, 1"
                                ));
                                format!(
                                    "%t{} = call i64 @rt_thread_spawn_closure(ptr {}, ptr {})",
                                    result, fn_ptr, env_ptr
                                )
                            } else {
                                let fn_ptr = if ty != "ptr" {
                                    let tmp = tmp_extra(ctx, "ptr");
                                    extra.push(format!("{tmp} = inttoptr {ty} {val} to ptr"));
                                    tmp
                                } else {
                                    val
                                };
                                format!(
                                    "%t{} = call i64 @rt_thread_spawn_closure(ptr {}, ptr null)",
                                    result, fn_ptr
                                )
                            }
                        }
                        _ => String::new(),
                    }
                }
            } else if name_opt == Some("thread.join") {
                if args.is_empty() {
                    let result = tmp_result(ctx, "i64", inst.result);
                    format!("%t{result} = call i64 @pal_thread_join(i64 0, ptr null)")
                } else {
                    let (tid_val, tid_ty) = resolve_typed(&args[0], ctx);
                    let ret_storage = tmp_extra(ctx, "ptr");
                    extra.push(format!("{ret_storage} = alloca ptr"));
                    extra.push(format!(
                        "call i64 @pal_thread_join({tid_ty} {tid_val}, ptr {ret_storage})"
                    ));
                    let ret_ptr = tmp_extra(ctx, "ptr");
                    extra.push(format!("{ret_ptr} = load ptr, ptr {ret_storage}"));
                    let result_id = tmp_result(ctx, "i64", inst.result);
                    format!("%t{result_id} = ptrtoint ptr {ret_ptr} to i64")
                }
            } else if let Some(llvm_name) = builtin_to_pal(name_opt.unwrap_or("")) {
                compile_pal_builtin(inst, args, llvm_name, ctx, &mut extra)
            } else if name_opt == Some("call") {
                // Indirect call via function pointer
                if args.is_empty() {
                    return InstCompile::Normal(vec![]);
                }
                let ll_ret = llvm_type_str(&ret_ty.data_type);
                let result = if ll_ret == "void" {
                    None
                } else {
                    Some(tmp_result(ctx, &ll_ret, inst.result))
                };
                let (fn_ptr, fn_ptr_ty) = resolve_typed(&args[0], ctx);
                // Ensure fn_ptr is ptr-typed for the bitcast
                let fn_ptr_final: String = if fn_ptr_ty != "ptr" {
                    let tmp = tmp_extra(ctx, "ptr");
                    extra.push(format!(
                        "{} = inttoptr {} {} to ptr",
                        tmp, fn_ptr_ty, fn_ptr
                    ));
                    tmp
                } else {
                    fn_ptr.clone()
                };
                let mut arg_strs = Vec::new();
                let mut param_tys = Vec::new();
                // Every indirect-call target is a mire function (possibly an extern wrapper),
                // so it always receives an implicit environment pointer as the first argument.
                // If the callee value carries an environment (e.g. a capturing closure),
                // pass it through; otherwise pass null.
                let env_arg = match &args[0] {
                    MirValue::FunctionRef { env, .. } => {
                        let (env_str, env_ty) = resolve_typed(env, ctx);
                        if env_ty == "ptr" {
                            format!("ptr {}", env_str)
                        } else {
                            let tmp = tmp_extra(ctx, "ptr");
                            extra.push(format!("{} = inttoptr {} {} to ptr", tmp, env_ty, env_str));
                            format!("ptr {}", tmp)
                        }
                    }
                    _ => "ptr null".to_string(),
                };
                arg_strs.push(env_arg);
                param_tys.push("ptr".to_string());
                for a in &args[1..] {
                    let (v, t) = resolve_typed(a, ctx);
                    if t == "i1" {
                        let zext = tmp_extra(ctx, "i64");
                        extra.push(format!("{} = zext i1 {} to i64", zext, v));
                        arg_strs.push(format!("i64 {}", zext));
                        param_tys.push("i64".to_string());
                    } else {
                        arg_strs.push(format!("{} {}", t, v));
                        param_tys.push(t.clone());
                    }
                }
                let fn_sig = format!("{} ({})*", ll_ret, param_tys.join(", "));
                let fn_cast = tmp_extra(ctx, "ptr");
                extra.push(format!(
                    "{} = bitcast ptr {} to {}",
                    fn_cast, fn_ptr_final, fn_sig
                ));
                match result {
                    Some(r) => format!(
                        "%t{} = call {} {}({})",
                        r,
                        ll_ret,
                        fn_cast,
                        arg_strs.join(", ")
                    ),
                    None => format!("call {} {}({})", ll_ret, fn_cast, arg_strs.join(", ")),
                }
            } else {
                // Regular function call — prefix user-defined functions with @fn_
                let is_void = matches!(ret_ty.data_type, DataType::None);
                let ll_ret: String = if is_void {
                    "void".to_string()
                } else {
                    llvm_type_str(&ret_ty.data_type)
                };
                let mut arg_strs = Vec::new();
                for a in args {
                    let (v, t) = resolve_typed(a, ctx);
                    if t == "i1" {
                        let zext = tmp_extra(ctx, "i64");
                        extra.push(format!("{} = zext i1 {} to i64", zext, v));
                        arg_strs.push(format!("i64 {}", zext));
                    } else {
                        arg_strs.push(format!("{} {}", t, v));
                    }
                }
                match callee {
                    MirValue::FunctionRef { name, env } => {
                        let is_str = matches!(ret_ty.data_type, DataType::Str);
                        resolve_named_call(
                            name,
                            env,
                            args,
                            &ll_ret,
                            is_void,
                            inst.result,
                            ctx,
                            &mut extra,
                            is_str,
                        )
                    }
                    MirValue::Global(name) => {
                        let is_str = matches!(ret_ty.data_type, DataType::Str);
                        resolve_named_call(
                            name,
                            &MirValue::Const(MirConst::None),
                            args,
                            &ll_ret,
                            is_void,
                            inst.result,
                            ctx,
                            &mut extra,
                            is_str,
                        )
                    }
                    MirValue::Temp(callee_id) => {
                        let (callee_val, callee_ty) =
                            resolve_typed(&MirValue::Temp(*callee_id), ctx);
                        if callee_ty == "{ ptr, ptr }" {
                            // Loaded closure value — extract fn_ptr + env_ptr via extractvalue
                            let fn_ptr = tmp_extra(ctx, "ptr");
                            let env_ptr = tmp_extra(ctx, "ptr");
                            extra.push(format!(
                                "{fn_ptr} = extractvalue {{ ptr, ptr }} {callee_val}, 0"
                            ));
                            extra.push(format!(
                                "{env_ptr} = extractvalue {{ ptr, ptr }} {callee_val}, 1"
                            ));
                            let mut param_tys: Vec<String> = vec!["ptr".to_string()];
                            arg_strs.insert(0, format!("ptr {env_ptr}"));
                            for a in args {
                                let (_, t) = resolve_typed(a, ctx);
                                param_tys.push(t.clone());
                            }
                            let fn_sig = format!("{} ({})*", ll_ret, param_tys.join(", "));
                            let fn_cast = tmp_extra(ctx, "ptr");
                            extra.push(format!("{fn_cast} = bitcast ptr {fn_ptr} to {fn_sig}"));
                            if is_void {
                                format!("call void {}({})", fn_cast, arg_strs.join(", "))
                            } else {
                                let r = tmp_result(ctx, &ll_ret, inst.result);
                                format!(
                                    "%t{r} = call {ll_ret} {fn_cast}({args_str})",
                                    args_str = arg_strs.join(", ")
                                )
                            }
                        } else {
                            // Alloca pointer — GEP into { ptr, ptr } struct to load fn_ptr + env_ptr
                            let fn_gep = tmp_extra(ctx, "ptr");
                            let env_gep = tmp_extra(ctx, "ptr");
                            let callee_name = ctx
                                .vars
                                .get(callee_id)
                                .cloned()
                                .unwrap_or_else(|| format!("%t{}", callee_id));
                            extra.push(format!(
                                "{fn_gep} = getelementptr {{ ptr, ptr }}, ptr {callee_name}, i32 0, i32 0"
                            ));
                            extra.push(format!(
                                "{env_gep} = getelementptr {{ ptr, ptr }}, ptr {callee_name}, i32 0, i32 1"
                            ));
                            let fn_ptr_loaded = tmp_extra(ctx, "ptr");
                            let env_ptr_loaded = tmp_extra(ctx, "ptr");
                            extra.push(format!("{fn_ptr_loaded} = load ptr, ptr {fn_gep}"));
                            extra.push(format!("{env_ptr_loaded} = load ptr, ptr {env_gep}"));
                            let mut param_tys: Vec<String> = vec!["ptr".to_string()];
                            arg_strs.insert(0, format!("ptr {env_ptr_loaded}"));
                            for a in args {
                                let (_, t) = resolve_typed(a, ctx);
                                param_tys.push(t.clone());
                            }
                            let fn_sig = format!("{} ({})*", ll_ret, param_tys.join(", "));
                            let fn_cast = tmp_extra(ctx, "ptr");
                            extra.push(format!(
                                "{fn_cast} = bitcast ptr {fn_ptr_loaded} to {fn_sig}"
                            ));
                            if is_void {
                                format!(
                                    "call void {fn_cast}({args_str})",
                                    args_str = arg_strs.join(", ")
                                )
                            } else {
                                let r = tmp_result(ctx, &ll_ret, inst.result);
                                format!(
                                    "%t{r} = call {ll_ret} {fn_cast}({args_str})",
                                    args_str = arg_strs.join(", ")
                                )
                            }
                        }
                    }
                    MirValue::EnvPtr | MirValue::Const(_) | MirValue::Param(_) => String::new(),
                }
            }
        }
        MirOp::Gep(base, indices, struct_name) => {
            let (base_str, _) = resolve_typed(base, ctx);
            let result = tmp_result(ctx, "ptr", inst.result);
            if struct_name == "rt_ptr_add" && indices.len() == 1 {
                let (index_value, index_type) = resolve_typed(&indices[0], ctx);
                let index = coerce_to(&index_value, &index_type, "i64", ctx, &mut extra);
                let offset_pointer = tmp_extra(ctx, "ptr");
                let is_null = tmp_extra(ctx, "i1");
                extra.push(format!(
                    "{} = getelementptr i8, ptr {}, i64 {}",
                    offset_pointer, base_str, index
                ));
                extra.push(format!("{} = icmp eq ptr {}, null", is_null, base_str));
                format!(
                    "%t{} = select i1 {}, ptr null, ptr {}",
                    result, is_null, offset_pointer
                )
            } else if struct_name == "rt_ptr_add_nonnull" && indices.len() == 1 {
                let (index_value, index_type) = resolve_typed(&indices[0], ctx);
                let index = coerce_to(&index_value, &index_type, "i64", ctx, &mut extra);
                format!(
                    "%t{} = getelementptr i8, ptr {}, i64 {}",
                    result, base_str, index
                )
            } else {
                let idx_strs: Vec<String> = indices
                    .iter()
                    .map(|i| {
                        let (v, t) = resolve_typed(i, ctx);
                        if t == "i64" && v.starts_with("%t") {
                            // Keep the index at 64 bits. Truncating to i32
                            // forced LLVM to re-extend it for every address
                            // computation (`trunc`/`sext` around a shift), and
                            // it silently wrapped indices past 2^31. Struct
                            // field indices are constants and stay i32 below,
                            // which is the only place LLVM requires it.
                            format!("i64 {}", v)
                        } else {
                            format!("i32 {}", v)
                        }
                    })
                    .collect();
                let struct_ty = if let Some(name) = struct_name.strip_prefix("struct:") {
                    ctx.struct_types
                        .get(name)
                        .map(|fields| render_struct_llvm_type(fields))
                        .unwrap_or_else(|| "ptr".to_string())
                } else if ctx.struct_types.contains_key(struct_name) {
                    render_struct_llvm_type(ctx.struct_types.get(struct_name).unwrap())
                } else {
                    struct_name.clone()
                };
                format!(
                    "%t{} = getelementptr inbounds {}, ptr {}, {}",
                    result,
                    struct_ty,
                    base_str,
                    idx_strs.join(", ")
                )
            }
        }
        MirOp::ZExt(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &llty, inst.result);
            format!("%t{} = zext {} {} to {}", result, src_t, v, llty)
        }
        MirOp::SExt(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &llty, inst.result);
            format!("%t{} = sext {} {} to {}", result, src_t, v, llty)
        }
        MirOp::Trunc(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = trunc {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::Copy(v) => {
            let (src, ty) = resolve_typed(v, ctx);
            let result = tmp_result(ctx, &ty, inst.result);
            format!(
                "%t{} = select i1 true, {} {}, {} {}",
                result, ty, src, ty, src
            )
        }
        MirOp::Sitofp(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            if src_t == dst_t {
                format!(
                    "%t{} = select i1 true, {} {}, {} {}",
                    result, src_t, v, src_t, v
                )
            } else if src_t == "ptr" && dst_t == "i64" {
                format!("%t{} = ptrtoint {} {} to {}", result, src_t, v, dst_t)
            } else if src_t == "i64" && dst_t == "ptr" {
                format!("%t{} = inttoptr {} {} to {}", result, src_t, v, dst_t)
            } else {
                format!("%t{} = sitofp {} {} to {}", result, src_t, v, dst_t)
            }
        }
        MirOp::Fptosi(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            if src_t == dst_t {
                format!(
                    "%t{} = select i1 true, {} {}, {} {}",
                    result, src_t, v, src_t, v
                )
            } else {
                format!("%t{} = fptosi {} {} to {}", result, src_t, v, dst_t)
            }
        }
        MirOp::Fptrunc(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = fptrunc {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::Fpext(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = fpext {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::PtrToInt(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = ptrtoint {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::IntToPtr(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = inttoptr {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::BitCast(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            format!("%t{} = bitcast {} {} to {}", result, src_t, v, dst_t)
        }
        MirOp::ExtractValue(agg, val, indices) => {
            let (a, at) = resolve_typed(agg, ctx);
            let (_v, _) = resolve_typed(val, ctx);
            // indices is a vec of field indices
            let idx_str = indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let result = tmp_result(ctx, &at, inst.result);
            format!("%t{} = extractvalue {} {}, {}", result, at, a, idx_str)
        }
        MirOp::InsertValue(agg, val, indices) => {
            let (a, at) = resolve_typed(agg, ctx);
            let (v, vt) = resolve_typed(val, ctx);
            let idx_str = indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let result = tmp_result(ctx, &at, inst.result);
            format!(
                "%t{} = insertvalue {} {}, {} {}, {}",
                result, at, a, vt, v, idx_str
            )
        }
        MirOp::Select(cond, t, f) => {
            let (c, _) = resolve_typed(cond, ctx);
            let (tv, tt) = resolve_typed(t, ctx);
            let (fv, ft) = resolve_typed(f, ctx);
            let result = tmp_result(ctx, &tt, inst.result);
            format!(
                "%t{} = select i1 {}, {} {}, {} {}",
                result, c, tt, tv, ft, fv
            )
        }
        MirOp::Drop(val, ownership) => {
            let (v, _) = resolve_typed(val, ctx);
            match ownership {
                Ownership::ManagedString => format!("call void @rt_managed_free(ptr {})", v),
                Ownership::List => format!("call void @rt_list_free(ptr {})", v),
                Ownership::Dict => format!("call void @rt_dict_free(ptr {})", v),
                Ownership::Borrowed | Ownership::None => String::new(),
            }
        }
        MirOp::Phi(_, _) => String::new(),
    };
    let mut result = Vec::with_capacity(extra.len() + 1);
    result.extend(extra);
    result.push(line);
    InstCompile::Normal(result)
}

// ── Inline vec runtime helpers ─────────────────────────────────────────

fn compile_vec_with_capacity_i64(
    _inst: &MirInst,
    ctx: &mut LlvmCtx,
    args: &[MirValue],
) -> InstCompile {
    let (cap_s, _) = resolve_typed(&args[0], ctx);
    let result = tmp_result(ctx, "ptr", _inst.result);
    InstCompile::Normal(vec![format!(
        "%t{} = call ptr @rt_vecs_with_capacity_i64(i64 {})",
        result, cap_s
    )])
}

fn compile_vec_len(_inst: &MirInst, ctx: &mut LlvmCtx, args: &[MirValue]) -> InstCompile {
    let (vec_s, _) = resolve_typed(&args[0], ctx);
    let result = tmp_result(ctx, "i64", _inst.result);
    // rt_vecs_len answers 0 for a null vector; inlining the load alone would
    // fault on one, so keep the guard as a select rather than a branch.
    let is_null = tmp_extra(ctx, "i1");
    let len_tmp = tmp_extra(ctx, "i64");
    InstCompile::Normal(vec![
        format!("{} = icmp eq ptr {}, null", is_null, vec_s),
        format!("{} = load i64, ptr {}", len_tmp, vec_s),
        format!(
            "%t{} = select i1 {}, i64 0, i64 {}",
            result, is_null, len_tmp
        ),
    ])
}

fn compile_vec_get_i64(
    _inst: &MirInst,
    ctx: &mut LlvmCtx,
    args: &[MirValue],
) -> InstCompile {
    let (vec_s, _) = resolve_typed(&args[0], ctx);
    let (idx_s, _) = resolve_typed(&args[1], ctx);
    let result = tmp_result(ctx, "i64", _inst.result);
    let len_tmp = tmp_extra(ctx, "i64");
    let ge0_tmp = tmp_extra(ctx, "i1");
    let lt_len_tmp = tmp_extra(ctx, "i1");
    let ok_tmp = tmp_extra(ctx, "i1");
    let data_ptr = tmp_extra(ctx, "ptr");
    let slot_tmp = tmp_extra(ctx, "ptr");
    let val_tmp = tmp_extra(ctx, "i64");
    let mut lines = Vec::new();
    lines.push(format!("{} = load i64, ptr {}", len_tmp, vec_s));
    lines.push(format!("{} = icmp sle i64 0, {}", ge0_tmp, idx_s));
    lines.push(format!(
        "{} = icmp slt i64 {}, {}",
        lt_len_tmp, idx_s, len_tmp
    ));
    lines.push(format!("{} = and i1 {}, {}", ok_tmp, ge0_tmp, lt_len_tmp));
    lines.push(format!(
        "{} = getelementptr ptr, ptr {}, i64 1",
        data_ptr, vec_s
    ));
    lines.push(format!(
        "{} = getelementptr i64, ptr {}, i64 {}",
        slot_tmp, data_ptr, idx_s
    ));
    lines.push(format!("{} = load i64, ptr {}", val_tmp, slot_tmp));
    lines.push(format!(
        "%t{} = select i1 {}, i64 {}, i64 0",
        result, ok_tmp, val_tmp
    ));
    InstCompile::Normal(lines)
}

// Releasing a vector, lowered inline.
//
// `rt_list_free` is a null-guarded `free` of the header word. Left as a call
// it is a frame plus a call on every release, and an allocation-heavy loop
// releases once per iteration; worse, the optimizer cannot fold the call away
// when the pointer is known to be null, because the callee is opaque. A
// program that re-initialises an owned local each iteration releases a null
// slot every time and paid a full call to say so.
//
// Lowering to a guarded `free` keeps the null check in the module, so a null
// release costs a compare and nothing else.
fn compile_vec_free(ctx: &mut LlvmCtx, args: &[MirValue]) -> InstCompile {
    let (vec_s, _) = resolve_typed(&args[0], ctx);

    let null_bb = ctx.next_extra;
    ctx.next_extra += 1;
    let free_bb = ctx.next_extra;
    ctx.next_extra += 1;
    let done_bb = ctx.next_extra;
    ctx.next_extra += 1;

    let null_label = format!("bb_free_null_{}", null_bb);
    let free_label = format!("bb_free_call_{}", free_bb);
    let done_label = format!("bb_free_done_{}", done_bb);
    // Defined without the sigil, referenced with it: see the note in
    // compile_vec_push_i64. The bare name in a `br` makes `opt` reject the
    // module with "expected value token".
    let null_ref = format!("%{}", null_label);
    let free_ref = format!("%{}", free_label);
    let done_ref = format!("%{}", done_label);

    let is_null = tmp_extra(ctx, "i1");
    let before = vec![format!(
        "{} = icmp eq ptr {}, null",
        is_null, vec_s
    )];
    let term = format!(
        "br i1 {}, label {}, label {}",
        is_null, null_ref, free_ref
    );

    // A list pointer addresses the length word, so the block the PAL owns is
    // one word below it.
    let hdr_tmp = tmp_extra(ctx, "ptr");
    let mut free_lines = vec![format!(
        "{} = getelementptr ptr, ptr {}, i64 -1",
        hdr_tmp, vec_s
    )];
    free_lines.push(format!("call void @free(ptr {})", hdr_tmp));

    InstCompile::Split {
        before,
        term,
        successors: vec![
            (null_label, Vec::new(), Some(format!("br label {}", done_ref))),
            (free_label, free_lines, Some(format!("br label {}", done_ref))),
            (done_label, Vec::new(), None),
        ],
    }
}

// `vec::push` for i64 elements, lowered inline instead of calling
// `rt_vecs_push_i64`.
//
// The vector layout is `list[-1] = capacity`, `list[0] = length`,
// `list[1..] = data`, so the capacity test needs one load from `vec - 8` and
// the store needs one from `vec + 8`. After lowering, the capacity branch is
// the same branch C emits:
//
//     if (len == cap) { grow; }
//     data[len++] = value;
//
// The reallocation is a real call to `rt_vecs_grow_push_i64`. That costs
// nothing measurable: a vector of n elements reallocates O(log n) times, so
// the branch is taken a handful of times per vector against n inline pushes.
// The alternative -- open-coding the realloc in IR -- multiplied basic blocks
// per push site for no gain and made every program that pushes in a loop
// carry the allocation path in its inner loop.
fn compile_vec_push_i64(
    inst: &MirInst,
    ctx: &mut LlvmCtx,
    args: &[MirValue],
) -> InstCompile {
    let (vec_s, _) = resolve_typed(&args[0], ctx);
    let (val_s, _) = resolve_typed(&args[1], ctx);
    let result = format!("%t{}", tmp_result(ctx, "ptr", inst.result));

    // `push_elide` proves that this push cannot exceed the capacity of the
    // vector it appends to (its allocation site reserved at least as many
    // elements as the push site can execute). In that case the capacity load,
    // the compare and the growth branch are all dead: append unconditionally.
    // The vector stays heap allocated; only the test disappears. On
    // `alloc_pressure` -- eight million fresh vectors of eight elements -- the
    // test is the entire gap against the equivalent C loop.
    let unchecked = inst
        .result
        .map(|r| ctx.unchecked_pushes.contains(&r))
        .unwrap_or(false);
    if unchecked {
        let data_tmp = tmp_extra(ctx, "ptr");
        let len_tmp = tmp_extra(ctx, "i64");
        let slot_tmp = tmp_extra(ctx, "ptr");
        let new_len_tmp = tmp_extra(ctx, "i64");
        let mut lines = Vec::new();
        lines.push(format!("{} = load i64, ptr {}", len_tmp, vec_s));
        lines.push(format!(
            "{} = getelementptr ptr, ptr {}, i64 1",
            data_tmp, vec_s
        ));
        lines.push(format!(
            "{} = getelementptr i64, ptr {}, i64 {}",
            slot_tmp, data_tmp, len_tmp
        ));
        lines.push(format!("store i64 {}, ptr {}", val_s, slot_tmp));
        lines.push(format!("{} = add i64 {}, 1", new_len_tmp, len_tmp));
        lines.push(format!("store i64 {}, ptr {}", new_len_tmp, vec_s));
        // The result is the same pointer the caller passed in. `getelementptr
        // ... i64 0` is a zero-offset copy of it; LLVM folds the two together.
        lines.push(format!(
            "{} = getelementptr i8, ptr {}, i64 0",
            result, vec_s
        ));
        return InstCompile::Normal(lines);
    }

    let slow_bb = ctx.next_extra;
    ctx.next_extra += 1;
    let fast_bb = ctx.next_extra;
    ctx.next_extra += 1;
    let merge_bb = ctx.next_extra;
    ctx.next_extra += 1;

    // A basic block is *defined* by an unquoted name (`bb_push_slow_0:`) but
    // *referenced* with a leading `%` (`br label %bb_push_slow_0`). Emitting
    // the bare name in a branch makes `opt` reject the module with
    // "expected value token", so the two spellings are kept apart here.
    let slow_label = format!("bb_push_slow_{}", slow_bb);
    let fast_label = format!("bb_push_fast_{}", fast_bb);
    let merge_label = format!("bb_push_merge_{}", merge_bb);
    let slow_ref = format!("%{}", slow_label);
    let fast_ref = format!("%{}", fast_label);
    let merge_ref = format!("%{}", merge_label);

    let hdr_tmp = tmp_extra(ctx, "ptr");
    let cap_tmp = tmp_extra(ctx, "i64");
    let len_tmp = tmp_extra(ctx, "i64");
    let full_tmp = tmp_extra(ctx, "i1");

    let mut before = Vec::new();
    before.push(format!(
        "{} = getelementptr ptr, ptr {}, i64 -1",
        hdr_tmp, vec_s
    ));
    before.push(format!("{} = load i64, ptr {}", cap_tmp, hdr_tmp));
    before.push(format!("{} = load i64, ptr {}", len_tmp, vec_s));
    before.push(format!(
        "{} = icmp sge i64 {}, {}",
        full_tmp, len_tmp, cap_tmp
    ));

    let term = format!(
        "br i1 {}, label {}, label {}",
        full_tmp, slow_ref, fast_ref
    );

    // Fast path: capacity available. Append and return the same pointer.
    let data_tmp = tmp_extra(ctx, "ptr");
    let slot_tmp = tmp_extra(ctx, "ptr");
    let new_len_tmp = tmp_extra(ctx, "i64");
    let mut fast_lines = Vec::new();
    fast_lines.push(format!(
        "{} = getelementptr ptr, ptr {}, i64 1",
        data_tmp, vec_s
    ));
    fast_lines.push(format!(
        "{} = getelementptr i64, ptr {}, i64 {}",
        slot_tmp, data_tmp, len_tmp
    ));
    fast_lines.push(format!("store i64 {}, ptr {}", val_s, slot_tmp));
    fast_lines.push(format!("{} = add i64 {}, 1", new_len_tmp, len_tmp));
    fast_lines.push(format!("store i64 {}, ptr {}", new_len_tmp, vec_s));

    // Slow path: reallocate and append. The call returns the new list pointer.
    let grown_tmp = tmp_extra(ctx, "ptr");
    let mut slow_lines = Vec::new();
    slow_lines.push(format!(
        "{} = call ptr @{}(ptr {}, i64 {})",
        grown_tmp, VEC_GROW_PUSH_I64, vec_s, val_s
    ));

    // Merge: the fast path keeps the incoming pointer, the slow path returns
    // the reallocated one.
    let mut merge_lines = Vec::new();
    merge_lines.push(format!(
        "{} = phi ptr [ {}, {} ], [ {}, {} ]",
        result, vec_s, fast_ref, grown_tmp, slow_ref
    ));

    InstCompile::Split {
        before,
        term,
        successors: vec![
            (
                fast_label,
                fast_lines,
                Some(format!("br label {}", merge_ref)),
            ),
            (
                slow_label,
                slow_lines,
                Some(format!("br label {}", merge_ref)),
            ),
            (merge_label, merge_lines, None),
        ],
    }
}
