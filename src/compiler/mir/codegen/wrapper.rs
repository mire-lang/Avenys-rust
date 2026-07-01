use super::sanitize_fn_name;
use super::types::llvm_type_str;
use crate::compiler::mir::{MirExternFunction, MirOp, MirProgram, MirValue};
use std::collections::HashSet;

pub(crate) fn collect_used_extern_wrappers(
    program: &MirProgram,
    extern_fn_names: &HashSet<String>,
) -> HashSet<String> {
    let mut used = HashSet::new();
    let mut visit_value = |v: &MirValue| match v {
        MirValue::Global(name) | MirValue::FunctionRef { name, .. }
            if extern_fn_names.contains(name) =>
        {
            used.insert(name.clone());
        }
        _ => {}
    };
    for func in &program.functions {
        for block in &func.blocks {
            for inst in &block.insts {
                match &inst.op {
                    MirOp::Call(callee, args, _) => {
                        visit_value(callee);
                        for arg in args {
                            visit_value(arg);
                        }
                    }
                    MirOp::Load(v, _)
                    | MirOp::Store(_, v)
                    | MirOp::Copy(v)
                    | MirOp::Add(v, _)
                    | MirOp::Sub(v, _)
                    | MirOp::Mul(v, _)
                    | MirOp::SDiv(v, _)
                    | MirOp::SRem(v, _)
                    | MirOp::Shl(v, _)
                    | MirOp::And(v, _)
                    | MirOp::Or(v, _)
                    | MirOp::ICmp(_, v, _)
                    | MirOp::FCmp(_, v, _)
                    | MirOp::Gep(v, _, _)
                    | MirOp::PtrToInt(v, _)
                    | MirOp::IntToPtr(v, _)
                    | MirOp::BitCast(v, _)
                    | MirOp::ZExt(v, _)
                    | MirOp::Trunc(v, _)
                    | MirOp::Sitofp(v, _)
                    | MirOp::Fptosi(v, _) => {
                        visit_value(v);
                        if let MirOp::Add(_, r)
                        | MirOp::Sub(_, r)
                        | MirOp::Mul(_, r)
                        | MirOp::SDiv(_, r)
                        | MirOp::SRem(_, r)
                        | MirOp::Shl(_, r)
                        | MirOp::And(_, r)
                        | MirOp::Or(_, r)
                        | MirOp::ICmp(_, _, r)
                        | MirOp::FCmp(_, _, r)
                        | MirOp::Store(r, _) = &inst.op
                        {
                            visit_value(r);
                        }
                        if let MirOp::Gep(_, indices, _) = &inst.op {
                            for idx in indices {
                                visit_value(idx);
                            }
                        }
                    }
                    MirOp::Phi(pairs, _) => {
                        for (v, _) in pairs {
                            visit_value(v);
                        }
                    }
                    MirOp::Select(c, t, f) => {
                        visit_value(c);
                        visit_value(t);
                        visit_value(f);
                    }
                    MirOp::Alloca(_) => {}
                }
            }
        }
    }
    used
}

pub(crate) fn generate_extern_wrapper(ext: &MirExternFunction) -> String {
    let wrapper_name = format!("@fn_{}_wrapper", sanitize_fn_name(&ext.name));
    let ret_ty = llvm_type_str(&ext.return_type);
    let param_tys: Vec<String> = ext.params.iter().map(llvm_type_str).collect();
    let param_names: Vec<String> = (0..ext.params.len())
        .map(|i| format!("%arg_{}", i))
        .collect();
    let param_strs: Vec<String> = param_tys
        .iter()
        .zip(param_names.iter())
        .map(|(t, n)| format!("{} {}", t, n))
        .collect();
    let mut lines = Vec::new();
    let sig_params = if param_strs.is_empty() {
        "ptr %env_ptr".to_string()
    } else {
        format!("ptr %env_ptr, {}", param_strs.join(", "))
    };
    lines.push(format!(
        "define {} {}({}) {{",
        ret_ty, wrapper_name, sig_params
    ));
    let arg_strs: Vec<String> = param_tys
        .iter()
        .zip(param_names.iter())
        .map(|(t, n)| format!("{} {}", t, n))
        .collect();
    if ret_ty == "void" {
        lines.push(format!(
            "  call void @{}({})",
            ext.name,
            arg_strs.join(", ")
        ));
        lines.push("  ret void".to_string());
    } else {
        lines.push(format!(
            "  %r = call {} @{}({})",
            ret_ty,
            ext.name,
            arg_strs.join(", ")
        ));
        lines.push(format!("  ret {} %r", ret_ty));
    }
    lines.push("}".to_string());
    lines.join("\n")
}
