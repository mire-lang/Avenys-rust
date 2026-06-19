use super::{LlvmCtx, tmp_extra, tmp_result, const_str, sanitize_fn_name};
use crate::compiler::mir::{MirValue, MirConst};

pub(crate) fn resolve_typed(val: &MirValue, ctx: &mut LlvmCtx) -> (String, String) {
    match val {
        MirValue::Const(c) => {
            let v = const_str(c, ctx);
            let t = match c {
                MirConst::Int(_) | MirConst::Char(_) | MirConst::None => "i64",
                MirConst::Float(_) => "double",
                MirConst::Bool(_) => "i1",
                MirConst::Str(_) => "ptr",
            };
            (v, t.to_string())
        }
        MirValue::Temp(id) => {
            let n = ctx.vars.get(id).cloned().unwrap_or_else(|| format!("%t{}", id));
            let t = ctx.temp_types.get(id).cloned().unwrap_or_else(|| "i64".to_string());
            (n, t)
        }
        MirValue::Param(name) => {
            let t = ctx.param_types.get(name).cloned().unwrap_or_else(|| "i64".to_string());
            (format!("%arg_{}", name), t)
        }
        MirValue::Global(name) => {
            let llvm_name = if let Some(wrapper) = ctx.extern_wrapper_names.get(name) {
                wrapper.clone()
            } else if ctx.defined_fn_names.contains(name) {
                format!("@fn_{}", sanitize_fn_name(name))
            } else {
                format!("@{}", name)
            };
            (llvm_name, "ptr".to_string())
        }
        MirValue::EnvPtr => ("%env_ptr".to_string(), "ptr".to_string()),
        MirValue::FunctionRef { name, .. } => {
            (format!("@fn_{}", sanitize_fn_name(name)), "ptr".to_string())
        }
    }
}

pub(crate) fn resolve_named_call(
    name: &str,
    env: &MirValue,
    args: &[MirValue],
    ll_ret: &str,
    is_void: bool,
    result_id: Option<usize>,
    ctx: &mut LlvmCtx,
    extra: &mut Vec<String>,
) -> String {
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
    let (fn_name, needs_env) = if let Some(wrapper) = ctx.extern_wrapper_names.get(name) {
        (wrapper.clone(), true)
    } else if ctx.defined_fn_names.contains(name) {
        (format!("@fn_{}", sanitize_fn_name(name)), true)
    } else if ctx.extern_fn_names.contains(name) {
        (format!("@{}", name), false)
    } else {
        // Try stripping root namespace (e.g. "async.pal_proc_exists" -> "pal_proc_exists")
        let stripped = name.split_once('.').and_then(|(_, rest)| {
            if ctx.defined_fn_names.contains(rest) {
                Some(format!("@fn_{}", sanitize_fn_name(rest)))
            } else if ctx.extern_fn_names.contains(rest) {
                Some(format!("@{}", rest))
            } else {
                None
            }
        });
        let llvm_name = stripped.unwrap_or_else(|| format!("@{}", name));
        let needs = llvm_name.starts_with("@fn_");
        (llvm_name, needs)
    };
    // Mire functions carry an implicit environment pointer as the first argument.
    if needs_env {
        let env_str = match env {
            MirValue::Const(MirConst::None) => "ptr null".to_string(),
            other => {
                let (v, t) = resolve_typed(other, ctx);
                if t == "ptr" {
                    format!("ptr {}", v)
                } else {
                    let tmp = tmp_extra(ctx, "ptr");
                    extra.push(format!("{} = inttoptr {} {} to ptr", tmp, t, v));
                    format!("ptr {}", tmp)
                }
            }
        };
        arg_strs.insert(0, env_str);
    }
    if is_void {
        format!("call void {}({})", fn_name, arg_strs.join(", "))
    } else {
        let result = tmp_result(ctx, ll_ret, result_id);
        format!(
            "%t{} = call {} {}({})",
            result, ll_ret, fn_name, arg_strs.join(", ")
        )
    }
}

pub(crate) fn coerce_to_bool(operand: &str, from_ty: &str, ctx: &mut LlvmCtx, extra: &mut Vec<String>) -> String {
    if from_ty == "i1" {
        return operand.to_string();
    }
    let conv = tmp_extra(ctx, "i1");
    extra.push(format!("{} = icmp ne {} {}, 0", conv, from_ty, operand));
    conv
}

pub(crate) fn coerce_to(operand: &str, from_ty: &str, to_ty: &str, ctx: &mut LlvmCtx, extra: &mut Vec<String>) -> String {
    if from_ty == to_ty {
        return operand.to_string();
    }
    if to_ty == "double" && from_ty == "i64" {
        let conv = tmp_extra(ctx, "i64");
        extra.push(format!("{} = sitofp i64 {} to double", conv, operand));
        return conv;
    }
    operand.to_string()
}
