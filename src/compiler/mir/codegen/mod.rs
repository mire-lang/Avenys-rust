use super::*;
use crate::canonical_fn_name;
use std::collections::{HashMap, HashSet};

use self::expr::compile_inst;
use self::resolve::resolve_typed;
use self::types::llvm_type_str;
use self::wrapper::{collect_used_extern_wrappers, generate_extern_wrapper};

use crate::avens::{collect_used_symbols, filter_pal_decls};

pub(crate) mod builtins;
pub(crate) mod expr;
pub(crate) mod resolve;
pub(crate) mod types;
pub(crate) mod validate;

pub(crate) use self::validate::find_first_undefined_call;
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
    pub(crate) _source_filename: String,
}

pub fn mir_to_llvm(program: &MirProgram) -> (String, Vec<(String, String)>) {
    mir_to_llvm_with_filename(program, "")
}

pub fn mir_to_llvm_with_filename(
    program: &MirProgram,
    source_filename: &str,
) -> (String, Vec<(String, String)>) {
    let mut extern_decls = Vec::new();
    let mut declared = std::collections::HashSet::new();
    for ext in &program.extern_functions {
        if !declared.insert(ext.name.clone()) {
            continue;
        }
        let ret = if matches!(ext.return_type, DataType::None) {
            "void".to_string()
        } else {
            llvm_type_str(&ext.return_type)
        };
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
        _source_filename: source_filename.to_string(),
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
    let target_triple = crate::avens::build_support::c_defs()
        .target
        .clone()
        .unwrap_or_else(|| "x86_64-unknown-linux-gnu".to_string());
    out.push(format!("target triple = \"{}\"", target_triple));
    out.push(String::new());
    out.extend(extern_decls);
    // Unified dependency collector: scan IR for used symbols, emit only needed declarations.
    // In `full` mode we still emit everything for backward-compatibility.
    let used = collect_used_symbols(&out.join("\n"));
    let runtime_tier = crate::avens::build_support::c_defs().runtime;
    out.extend(filter_pal_decls(&used.pal, runtime_tier));
    out.push(String::new());
    for (name, ty) in &program.globals {
        out.push(format!(
            "@{} = global {} zeroinitializer",
            name,
            llvm_type_str(ty)
        ));
    }
    out.push(String::new());
    out.extend(strings);
    out.extend(function_irs);

    (out.join("\n"), program.extern_libs.clone())
}

fn sanitize_fn_name(name: &str) -> String {
    let name = canonical_fn_name(name);
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
    let ret_type = if matches!(func.ret_type, DataType::None) {
        "void".to_string()
    } else {
        llvm_type_str(&func.ret_type)
    };
    let saved_vars = std::mem::take(&mut ctx.vars);
    let saved_temp_types = std::mem::take(&mut ctx.temp_types);
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

    // LLVM re-executes `alloca` at runtime, so allocas inside loops allocate
    // stack every iteration and are never freed until the function returns.
    // Hoist every alloca to the entry block so loop-local locals allocate once.
    let mut hoisted_allocas: Vec<String> = Vec::new();
    for block in &func.blocks {
        for inst in &block.insts {
            if matches!(inst.op, MirOp::Alloca(_)) {
                for line in compile_inst(inst, ctx) {
                    if !line.is_empty() {
                        hoisted_allocas.push(format!("  {}", line));
                    }
                }
            }
        }
    }

    for block in &func.blocks {
        if block.id > 0 {
            parts.push(String::new());
        }
        parts.push(format!("bb_{}:", block.id));

        if block.id == 0 {
            parts.extend(hoisted_allocas.iter().cloned());
        }

        for inst in &block.insts {
            if matches!(inst.op, MirOp::Alloca(_)) {
                continue;
            }
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
                    _ => {
                        let mut buf = [0u8; 4];
                        c.encode_utf8(&mut buf)
                            .bytes()
                            .flat_map(|b| format!("\\{:02X}", b).chars().collect::<Vec<char>>())
                            .collect()
                    }
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
        MirConst::Struct { .. } => "zeroinitializer".to_string(),
        MirConst::Zero { .. } => "zeroinitializer".to_string(),
    }
}

fn default_return_for_type(ret_type: &str) -> String {
    match ret_type {
        "void" => "ret void".to_string(),
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
        MirTerminator::Ret(None) => "ret void".to_string(),
        MirTerminator::Unreachable => "unreachable".to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod flush_order_tests {
    use super::mir_to_llvm;
    use crate::compiler::mir::{
        DataType, MirBlock, MirConst, MirFunction, MirInst, MirOp, MirProgram, MirTerminator,
        MirType, MirValue,
    };
    use std::collections::HashMap;

    /// Builds a one-function program whose body is a single call to `name` with
    /// one i64 constant argument, and returns the generated LLVM IR.
    fn ir_for_call(name: &str) -> String {
        let program = MirProgram {
            functions: vec![MirFunction {
                name: "main".to_string(),
                params: Vec::new(),
                ret_type: DataType::None,
                body_hash: 0,
                noinline: false,
                blocks: vec![MirBlock {
                    id: 0,
                    label: "entry".to_string(),
                    insts: vec![MirInst {
                        result: Some(0),
                        op: MirOp::Call(
                            MirValue::Global(name.to_string()),
                            vec![MirValue::Const(MirConst::Int(7))],
                            MirType {
                                data_type: DataType::I64,
                            },
                        ),
                        loc: (1, 1),
                    }],
                    terminator: MirTerminator::Ret(None),
                }],
            }],
            entry_point: Some("main".to_string()),
            extern_functions: Vec::new(),
            extern_libs: Vec::new(),
            struct_types: HashMap::new(),
            globals: HashMap::new(),
        };
        let (ir, _strings) = mir_to_llvm(&program);
        ir
    }

    /// Index of the first `needle` in `haystack`.
    fn at(haystack: &str, needle: &str) -> usize {
        haystack
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` missing from the generated IR"))
    }

    #[test]
    fn dasu_flushes_after_printing() {
        let ir = ir_for_call("dasu");
        let printf = at(&ir, "@printf");
        let flush = at(&ir, "@fflush");
        assert!(
            printf < flush,
            "dasu must printf before it flushes, otherwise the flush only pushes \
             out the previous line and this one waits in the buffer.\n{ir}"
        );
    }

    #[test]
    fn ireru_flushes_after_writing_the_prompt() {
        let ir = ir_for_call("ireru");
        let prompt = at(&ir, "@ireru");
        let flush = at(&ir, "@fflush");
        assert!(
            prompt < flush,
            "ireru must write the prompt before it flushes, otherwise the reader \
             is prompted by a flush that had nothing to send yet.\n{ir}"
        );
    }

    #[test]
    fn dasu_still_emits_exactly_one_flush_per_call() {
        // The fix moved the flush from the returned line into `extra`, which is
        // emitted first. A second flush would mean the sequence got emitted twice
        // rather than reordered.
        let ir = ir_for_call("dasu");
        assert_eq!(
            ir.matches("@fflush").count(),
            1,
            "one dasu call must produce one flush:\n{ir}"
        );
    }
}
