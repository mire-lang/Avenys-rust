use super::builtins::{builtin_to_pal, compile_pal_builtin};
use super::resolve::{coerce_to, coerce_to_bool, resolve_named_call, resolve_typed};
use super::types::{llvm_type_str, render_struct_llvm_type};
use super::{LlvmCtx, sanitize_fn_name, tmp_extra, tmp_result};
use crate::compiler::mir::{DataType, MirCmp, MirConst, MirInst, MirOp, MirValue};

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

pub(crate) fn compile_inst(inst: &MirInst, ctx: &mut LlvmCtx) -> Vec<String> {
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
                // If left operand is a temporary, it's likely an intermediate in a concat chain
                // and can be freed after use
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
            format!("%t{} = lshr i64 {}, {}", result, l, r)
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
                // Ensure both sides are ptr
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
                // Use strcmp for Eq/Ne on pointer types (strings)
                let scmp = tmp_extra(ctx, "i32");
                extra.push(format!(
                    "{} = call i32 @strcmp(ptr {}, ptr {})",
                    scmp, l_ptr, r_ptr
                ));
                format!("%t{} = icmp {} i32 {}, 0", result, cond, scmp)
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
                    return vec![];
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
                    return vec![];
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
                extra.push(format!("%t{} = call ptr @ireru(ptr {})", result, prompt_ptr));
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
                    return vec![];
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
            let idx_strs: Vec<String> = indices
                .iter()
                .map(|i| {
                    let (v, t) = resolve_typed(i, ctx);
                    if t == "i64" && v.starts_with("%t") {
                        let trunc = tmp_extra(ctx, "i32");
                        extra.push(format!("{} = trunc i64 {} to i32", trunc, v));
                        format!("i32 {}", trunc)
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
        MirOp::Drop(val) => {
            // Materialize the Drop: tier-aware lowering
            let (v, t) = resolve_typed(val, ctx);
            if t == "ptr" {
                // String/vec/map/closure — needs runtime free in minimal, no-op in full, free in none
                format!("call void @rt_managed_free(ptr {})", v)
            } else {
                // Primitive types (i64, f64, bool, etc.) — no runtime action needed
                String::new()
            }
        }
        MirOp::Phi(_, _) => String::new(),
    };
    let mut result = Vec::with_capacity(extra.len() + 1);
    result.extend(extra);
    result.push(line);
    result
}
