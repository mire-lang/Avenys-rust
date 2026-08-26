use crate::compiler::mir::{MirOp, MirProgram, MirValue};

use super::builtins::{builtin_to_pal, pal_extern_decls};
use std::collections::HashSet;

const SPECIAL_BARE_CALLS: &[&str] = &[
    "str",
    "dasu",
    "print",
    "ireru",
    "env_args",
    "call",
    "__if_expr",
    "__do_while",
    "range",
    "len",
    "contains",
];

fn pal_decl_names() -> HashSet<String> {
    let mut names = HashSet::new();
    for decl in pal_extern_decls() {
        if let Some(rest) = decl.strip_prefix("declare ") {
            if let Some(start) = rest.find('@') {
                let after = &rest[start + 1..];
                if let Some(end) = after.find('(') {
                    names.insert(after[..end].trim().to_string());
                }
            }
        }
    }
    names
}

pub(crate) fn find_first_undefined_call(
    program: &MirProgram,
) -> Option<(String, (usize, usize))> {
    let defined: HashSet<String> =
        program.functions.iter().map(|f| f.name.clone()).collect();
    let extern_names: HashSet<String> =
        program.extern_functions.iter().map(|e| e.name.clone()).collect();
    let struct_names: HashSet<String> = program.struct_types.keys().cloned().collect();
    let pal_decls = pal_decl_names();
    for func in &program.functions {
        for block in &func.blocks {
            for inst in &block.insts {
                let name = match &inst.op {
                    MirOp::Call(callee, _, _) => match callee {
                        MirValue::Global(n) => Some(n.as_str()),
                        MirValue::FunctionRef { name, .. } => Some(name.as_str()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(n) = name {
                    if n.contains('.') || n.contains(':') {
                        continue;
                    }
                    if n.starts_with("rt_")
                        || n.starts_with("pal_")
                        || n.starts_with("fn_")
                        || n.starts_with("alloca_")
                    {
                        continue;
                    }
                    if SPECIAL_BARE_CALLS.contains(&n) {
                        continue;
                    }
                    if builtin_to_pal(n).is_some() {
                        continue;
                    }
                    if defined.contains(n)
                        || extern_names.contains(n)
                        || pal_decls.contains(n)
                        || struct_names.contains(n)
                    {
                        continue;
                    }
                    return Some((n.to_string(), inst.loc));
                }
            }
        }
    }
    None
}
