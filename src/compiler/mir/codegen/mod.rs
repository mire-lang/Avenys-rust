use super::*;
use std::collections::{HashMap, HashSet};

use self::builtins::pal_extern_decls;
use self::expr::compile_inst;
use self::resolve::resolve_typed;
use self::types::llvm_type_str;
use self::wrapper::{collect_used_extern_wrappers, generate_extern_wrapper};

pub(crate) mod builtins;
pub(crate) mod expr;
pub(crate) mod resolve;
pub(crate) mod types;
pub(crate) mod wrapper;

pub(crate) struct LlvmCtx<'a> {
    pub(crate) strings: Vec<String>,
    vars: HashMap<usize, String>,
    temp_types: HashMap<usize, String>,
    param_types: HashMap<String, String>,
    next_tmp: usize,
    next_extra: usize,
    next_string_id: usize,
    defined_fn_names: HashSet<String>,
    extern_fn_names: HashSet<String>,
    /// Maps extern function name -> its mire-wrapper LLVM name (e.g. "abs" -> "@fn_abs_wrapper").
    extern_wrapper_names: HashMap<String, String>,
    struct_types: &'a HashMap<String, Vec<(String, DataType)>>,
    /// Temp IDs that own heap-allocated strings (results of rt_string_concat, pal calls, etc.).
    /// Freed when consumed by another concat or stored to a variable.
    pub(crate) owned_string_temps: HashSet<usize>,
}

pub fn mir_to_llvm(program: &MirProgram) -> (String, Vec<(String, String)>) {
    let mut extern_decls = Vec::new();
    let mut declared = std::collections::HashSet::new();
    for ext in &program.extern_functions {
        if !declared.insert(ext.name.clone()) {
            continue;
        }
        let ret = llvm_type_str(&ext.return_type);
        let params: Vec<String> = ext.params.iter().map(llvm_type_str).collect();
        let sig = params.join(", ");
        extern_decls.push(format!("declare {} @{}({})", ret, ext.name, sig));
    }

    let mut function_irs = Vec::new();
    let mut defined_fn_names = std::collections::HashSet::new();
    let mut extern_fn_names = std::collections::HashSet::new();
    for func in &program.functions {
        defined_fn_names.insert(func.name.clone());
    }
    for ext in &program.extern_functions {
        extern_fn_names.insert(ext.name.clone());
    }
    let used_extern_wrappers = collect_used_extern_wrappers(program, &extern_fn_names);
    let extern_wrapper_names: HashMap<String, String> = used_extern_wrappers
        .iter()
        .map(|name| {
            (
                name.clone(),
                format!("@fn_{}_wrapper", sanitize_fn_name(name)),
            )
        })
        .collect();
    let mut ctx = LlvmCtx {
        strings: Vec::new(),
        vars: HashMap::new(),
        temp_types: HashMap::new(),
        param_types: HashMap::new(),
        next_tmp: 0,
        next_extra: 0,
        next_string_id: 0,
        defined_fn_names,
        extern_fn_names,
        extern_wrapper_names,
        struct_types: &program.struct_types,
        owned_string_temps: HashSet::new(),
    };
    for func in &program.functions {
        let func_ir = compile_function_to_llvm(func, &mut ctx);
        function_irs.push(func_ir);
    }
    // Generate wrappers only for extern functions that are actually used as function values.
    let ext_by_name: HashMap<String, &MirExternFunction> = program
        .extern_functions
        .iter()
        .map(|ext| (ext.name.clone(), ext))
        .collect();
    for name in &used_extern_wrappers {
        if let Some(&ext) = ext_by_name.get(name) {
            function_irs.push(generate_extern_wrapper(ext));
        }
    }
    let strings = ctx.strings;

    let mut out = Vec::new();
    out.push("target triple = \"x86_64-unknown-linux-gnu\"".to_string());
    out.push(String::new());
    out.extend(extern_decls);
    out.extend(pal_extern_decls());
    out.push(String::new());
    out.extend(strings);
    out.push(String::new());
    out.extend(function_irs);

    (out.join("\n"), program.extern_libs.clone())
}

fn sanitize_fn_name(name: &str) -> String {
    name.split_once('[')
        .map(|(base, rest)| {
            // base = "Box", rest = "T].get" → "Box.get"
            if let Some((_, after_bracket)) = rest.split_once(']') {
                format!("{}{}", base, after_bracket)
            } else {
                base.to_string()
            }
        })
        .unwrap_or_else(|| name.to_string())
}

pub(crate) fn compile_function_to_llvm(func: &MirFunction, ctx: &mut LlvmCtx) -> String {
    let llvm_name = format!("@fn_{}", sanitize_fn_name(&func.name));
    let ret_type = llvm_type_str(&func.ret_type);
    let saved_vars = std::mem::take(&mut ctx.vars);
    let saved_temp_types = std::mem::take(&mut ctx.temp_types);
    let saved_owned_string_temps = std::mem::take(&mut ctx.owned_string_temps);
    let saved_next_tmp = ctx.next_tmp;
    let saved_next_extra = ctx.next_extra;

    let mut param_strs = Vec::new();
    ctx.param_types.clear();
    ctx.next_tmp = 0;
    ctx.next_extra = 0;
    // Every mire function receives an implicit environment pointer as its first argument.
    param_strs.push("ptr %env_ptr".to_string());
    for p in &func.params {
        let ty = llvm_type_str(&p.data_type);
        let arg_n = format!("%arg_{}", p.name);
        ctx.param_types.insert(p.name.clone(), ty.clone());
        param_strs.push(format!("{} {}", ty, arg_n));
    }

    let mut parts = Vec::new();
    let noinline_attr = if func.noinline { " noinline" } else { "" };
    parts.push(format!(
        "define {} {}({}){} {{",
        ret_type,
        llvm_name,
        param_strs.join(", "),
        noinline_attr
    ));

    for block in &func.blocks {
        if block.id > 0 {
            parts.push(String::new());
        }
        parts.push(format!("bb_{}:", block.id));

        for inst in &block.insts {
            for line in compile_inst(inst, ctx) {
                if !line.is_empty() {
                    parts.push(format!("  {}", line));
                }
            }
        }

        // Any block that is still unreachable after lowering/inlining represents a
        // fall-off-the-end path; emit a default return so control cannot fall through
        // into later blocks (which the LLVM inliner or our own lowering may append
        // after the original last block).
        let term = if matches!(block.terminator, MirTerminator::Unreachable) {
            default_return_for_type(&ret_type)
        } else {
            compile_terminator(&block.terminator, ctx, &ret_type)
        };
        if !term.is_empty() {
            parts.push(format!("  {}", term));
        }
    }

    parts.push("}".to_string());
    ctx.vars = saved_vars;
    ctx.temp_types = saved_temp_types;
    ctx.owned_string_temps = saved_owned_string_temps;
    ctx.next_tmp = saved_next_tmp;
    ctx.next_extra = saved_next_extra;
    ctx.param_types.clear();
    parts.join("\n")
}

fn tmp_extra(ctx: &mut LlvmCtx, _ty: &str) -> String {
    let id = ctx.next_extra;
    ctx.next_extra += 1;
    format!("%e{}", id)
}

fn tmp_result(ctx: &mut LlvmCtx, ty: &str, mir_id: Option<usize>) -> usize {
    const EXTRA_TMP_OFFSET: usize = 100_000;
    let id = mir_id.unwrap_or_else(|| {
        let eid = ctx.next_extra;
        ctx.next_extra += 1;
        EXTRA_TMP_OFFSET + eid
    });
    let name = format!("%t{}", id);
    ctx.vars.insert(id, name);
    ctx.temp_types.insert(id, ty.to_string());
    id
}

fn const_str(c: &MirConst, ctx: &mut LlvmCtx) -> String {
    match c {
        MirConst::Int(v) => format!("{}", v),
        MirConst::Float(v) => {
            let bits = v.to_bits();
            format!("{:#x}", bits)
        }
        MirConst::Bool(v) => if *v { "1" } else { "0" }.to_string(),
        MirConst::Char(c) => format!("{}", *c as u32),
        MirConst::Str(s) => {
            let id = ctx.next_string_id;
            ctx.next_string_id += 1;
            let escaped = s
                .chars()
                .flat_map(|c| match c {
                    '\\' => "\\\\".chars().collect(),
                    '\n' => "\\0A".chars().collect(),
                    '\r' => "\\0D".chars().collect(),
                    '\t' => "\\09".chars().collect(),
                    '"' => "\\22".chars().collect(),
                    '\0' => "\\00".chars().collect(),
                    c if c.is_ascii_graphic() || c == ' ' => vec![c],
                    _ => format!("\\{:02X}", c as u8).chars().collect(),
                })
                .collect::<String>();
            let len = s.len() + 1;
            ctx.strings.push(format!(
                "@.str_{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                id, len, escaped
            ));
            format!("@.str_{}", id)
        }
        MirConst::None => "0".to_string(),
    }
}

fn default_return_for_type(ret_type: &str) -> String {
    match ret_type {
        "ptr" => "ret ptr null".to_string(),
        "double" => "ret double 0.0".to_string(),
        "float" => "ret float 0.0".to_string(),
        "i1" => "ret i1 0".to_string(),
        _ => format!("ret {} 0", ret_type),
    }
}

fn compile_terminator(term: &MirTerminator, ctx: &mut LlvmCtx, ret_type: &str) -> String {
    match term {
        MirTerminator::Br(target) => {
            format!("br label %bb_{}", target)
        }
        MirTerminator::BrCond(cond, t, f) => {
            let (c, _) = resolve_typed(cond, ctx);
            format!("br i1 {}, label %bb_{}, label %bb_{}", c, t, f)
        }
        MirTerminator::Ret(Some(val)) => {
            if matches!(val, &MirValue::Const(MirConst::None)) {
                return default_return_for_type(ret_type);
            }
            let (v, t) = resolve_typed(val, ctx);
            format!("ret {} {}", t, v)
        }
        MirTerminator::Ret(None) => default_return_for_type(ret_type),
        MirTerminator::Unreachable => "unreachable".to_string(),
    }
}
