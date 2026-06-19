use super::*;
use std::collections::HashMap;

pub(crate) struct LlvmCtx<'a> {
    pub(crate) strings: Vec<String>,
    vars: HashMap<usize, String>,
    temp_types: HashMap<usize, String>,
    param_types: HashMap<String, String>,
    next_tmp: usize,
    next_extra: usize,
    next_string_id: usize,
    defined_fn_names: std::collections::HashSet<String>,
    extern_fn_names: std::collections::HashSet<String>,
    /// Maps extern function name -> its mire-wrapper LLVM name (e.g. "abs" -> "@fn_abs_wrapper").
    extern_wrapper_names: HashMap<String, String>,
    struct_types: &'a HashMap<String, Vec<(String, DataType)>>,
}

pub fn mir_to_llvm(program: &MirProgram) -> (String, Vec<(String, String)>) {
    let mut extern_decls = Vec::new();
    let mut declared = std::collections::HashSet::new();
    for ext in &program.extern_functions {
        if !declared.insert(ext.name.clone()) {
            continue;
        }
        let ret = llvm_type_str(&ext.return_type);
        let params: Vec<String> = ext.params.iter().map(|t| llvm_type_str(t)).collect();
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
        .map(|name| (name.clone(), format!("@fn_{}_wrapper", sanitize_fn_name(name))))
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
    out.push("declare ptr @rt_get_args(i32, ptr)".to_string());
    out.push("declare ptr @rt_bool_to_string(i64)".to_string());
    out.push("declare ptr @rt_f64_to_string(double)".to_string());
    out.push("declare ptr @rt_i64_to_string(i64)".to_string());
    out.push("declare i64 @abs(i64)".to_string());
    out.push("declare double @rt_math_sqrt(double)".to_string());
    out.push("declare double @rt_math_pow(double, double)".to_string());
    out.push("declare i64 @rt_math_round(double)".to_string());
    out.push("declare i64 @rt_math_floor(double)".to_string());
    out.push("declare i64 @rt_math_ceil(double)".to_string());
    out.push("declare i64 @pal_fs_delete(ptr)".to_string());
    out.push("declare i64 @pal_fs_write(ptr, ptr)".to_string());
    out.push("declare i64 @pal_fs_append(ptr, ptr)".to_string());
    out.push("declare ptr @pal_fs_read(ptr)".to_string());
    out.push("declare i64 @pal_fs_copy(ptr, ptr)".to_string());
    out.push("declare i64 @pal_fs_move(ptr, ptr)".to_string());
    out.push("declare i64 @pal_fs_mkdir(ptr)".to_string());
    out.push("declare i64 @pal_fs_rmdir(ptr)".to_string());
    out.push("declare i64 @pal_fs_exists(ptr)".to_string());
    out.push("declare i64 @pal_fs_is_dir(ptr)".to_string());
    out.push("declare i64 @pal_fs_size(ptr)".to_string());
    out.push("declare ptr @pal_fs_list(ptr)".to_string());
    out.push("declare ptr @pal_fs_join(ptr, ptr)".to_string());
    out.push("declare ptr @pal_fs_dirname(ptr)".to_string());
    out.push("declare ptr @pal_fs_filename(ptr)".to_string());
    out.push("declare ptr @pal_fs_filext(ptr)".to_string());
    out.push("declare ptr @pal_proc_run(ptr)".to_string());
    out.push("declare ptr @pal_proc_exec(ptr)".to_string());
    out.push("declare i64 @pal_proc_spawn(ptr)".to_string());
    out.push("declare i64 @pal_proc_waitpid(i64)".to_string());
    out.push("declare i64 @pal_proc_kill(i64)".to_string());
    out.push("declare void @pal_proc_exit(i64)".to_string());
    out.push("declare i64 @pal_proc_exists(i64)".to_string());
    out.push("declare ptr @pal_env_get(ptr)".to_string());
    out.push("declare i32 @pal_env_set(ptr, ptr)".to_string());
    out.push("declare ptr @pal_env_cwd()".to_string());
    out.push("declare ptr @pal_env_all()".to_string());
    out.push(String::new());
    out.extend(strings);
    out.push(String::new());
    out.extend(function_irs);

    (out.join("\n"), Vec::new())
}

fn collect_used_extern_wrappers(
    program: &MirProgram,
    extern_fn_names: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let mut used = std::collections::HashSet::new();
    let mut visit_value = |v: &MirValue| {
        match v {
            MirValue::Global(name) | MirValue::FunctionRef { name, .. } => {
                if extern_fn_names.contains(name) {
                    used.insert(name.clone());
                }
            }
            _ => {}
        }
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
                    MirOp::Load(v, _) | MirOp::Store(_, v) | MirOp::Copy(v)
                    | MirOp::Add(v, _) | MirOp::Sub(v, _) | MirOp::Mul(v, _) | MirOp::SDiv(v, _)
                    | MirOp::SRem(v, _) | MirOp::Shl(v, _) | MirOp::And(v, _) | MirOp::Or(v, _)
                    | MirOp::ICmp(_, v, _) | MirOp::FCmp(_, v, _) | MirOp::Gep(v, _, _)
                    | MirOp::PtrToInt(v, _) | MirOp::IntToPtr(v, _) | MirOp::BitCast(v, _)
                    | MirOp::ZExt(v, _) | MirOp::Trunc(v, _) | MirOp::Sitofp(v, _)
                    | MirOp::Fptosi(v, _) => {
                        visit_value(v);
                        if let MirOp::Add(_, r) | MirOp::Sub(_, r) | MirOp::Mul(_, r)
                            | MirOp::SDiv(_, r) | MirOp::SRem(_, r) | MirOp::Shl(_, r)
                            | MirOp::And(_, r) | MirOp::Or(_, r) | MirOp::ICmp(_, _, r)
                            | MirOp::FCmp(_, _, r) | MirOp::Store(r, _) = &inst.op
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

fn sanitize_fn_name(name: &str) -> String {
    name.split_once('[').map(|(base, rest)| {
        // base = "Box", rest = "T].get" → "Box.get"
        if let Some((_, after_bracket)) = rest.split_once(']') {
            format!("{}{}", base, after_bracket)
        } else {
            base.to_string()
        }
    }).unwrap_or_else(|| name.to_string())
}

pub(crate) fn compile_function_to_llvm(func: &MirFunction, ctx: &mut LlvmCtx) -> String {
    let llvm_name = format!("@fn_{}", sanitize_fn_name(&func.name));
    let ret_type = llvm_type_str(&func.ret_type);
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
    ctx.next_tmp = saved_next_tmp;
    ctx.next_extra = saved_next_extra;
    ctx.param_types.clear();
    parts.join("\n")
}

fn generate_extern_wrapper(ext: &MirExternFunction) -> String {
    let wrapper_name = format!("@fn_{}_wrapper", sanitize_fn_name(&ext.name));
    let ret_ty = llvm_type_str(&ext.return_type);
    let param_tys: Vec<String> = ext.params.iter().map(llvm_type_str).collect();
    let param_names: Vec<String> = (0..ext.params.len()).map(|i| format!("%arg_{}", i)).collect();
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
        lines.push(format!("  call void @{}({})", ext.name, arg_strs.join(", ")));
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

fn llvm_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64".to_string(),
        DataType::I32 | DataType::U32 => "i32".to_string(),
        DataType::I16 | DataType::U16 => "i16".to_string(),
        DataType::I8 | DataType::U8 => "i8".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::None => "i64".to_string(),
        DataType::Array { element_type, size } => {
            format!("[{} x {}]", size, llvm_type_str(element_type))
        }
        DataType::Slice { element_type } => llvm_type_str(element_type),
        DataType::EnumNamed(_) => "i64".to_string(),
        DataType::Generic(_) => "i64".to_string(),
        _ => "ptr".to_string(),
    }
}

fn const_str(c: &MirConst, ctx: &mut LlvmCtx) -> String {
    match c {
        MirConst::Int(v) => format!("{}", v),
        MirConst::Float(v) => {
            let bits = v.to_bits();
            format!("{:#x}", bits)
        }
        MirConst::Bool(v) => {
            if *v { "1" } else { "0" }.to_string()
        }
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

fn resolve_typed(val: &MirValue, ctx: &mut LlvmCtx) -> (String, String) {
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

fn resolve_named_call(
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

fn coerce_to_bool(operand: &str, from_ty: &str, ctx: &mut LlvmCtx, extra: &mut Vec<String>) -> String {
    if from_ty == "i1" {
        return operand.to_string();
    }
    let conv = tmp_extra(ctx, "i1");
    extra.push(format!("{} = icmp ne {} {}, 0", conv, from_ty, operand));
    conv
}

fn coerce_to(operand: &str, from_ty: &str, to_ty: &str, ctx: &mut LlvmCtx, extra: &mut Vec<String>) -> String {
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

fn render_struct_llvm_type(fields: &[(String, DataType)]) -> String {
    let tys: Vec<String> = fields
        .iter()
        .map(|(_, dt)| llvm_type_str(dt))
        .collect();
    format!("{{ {} }}", tys.join(", "))
}

/// Maps a builtin function name to its PAL/LLVM callee name.
fn builtin_to_pal(name: &str) -> Option<&'static str> {
    match name {
        // Math
        "abs" => Some("abs"),
        "sqrt" => Some("rt_math_sqrt"),
        "pow" => Some("rt_math_pow"),
        "round" => Some("rt_math_round"),
        "floor" => Some("rt_math_floor"),
        "ceil" => Some("rt_math_ceil"),
        // Filesystem
        "fs_write" => Some("pal_fs_write"),
        "fs_append" => Some("pal_fs_append"),
        "fs_read" => Some("pal_fs_read"),
        "fs_copy" => Some("pal_fs_copy"),
        "fs_move" => Some("pal_fs_move"),
        "fs_drop" => Some("pal_fs_delete"),
        "fs_mkdir" => Some("pal_fs_mkdir"),
        "fs_rmdir" => Some("pal_fs_rmdir"),
        "fs_exists" => Some("pal_fs_exists"),
        "fs_is_dir" => Some("pal_fs_is_dir"),
        "fs_size" => Some("pal_fs_size"),
        "fs_list" => Some("pal_fs_list"),
        "fs_join" => Some("pal_fs_join"),
        "fs_dir" => Some("pal_fs_dirname"),
        "fs_name" => Some("pal_fs_filename"),
        "fs_ext" => Some("pal_fs_filext"),
        // Process
        "proc_run" => Some("pal_proc_run"),
        "proc_exec" => Some("pal_proc_exec"),
        "proc_spawn" => Some("pal_proc_spawn"),
        "proc_wait" => Some("pal_proc_waitpid"),
        "proc_kill" => Some("pal_proc_kill"),
        "proc_exit" => Some("pal_proc_exit"),
        "proc_exists" => Some("pal_proc_exists"),
        // Environment
        "env_get" => Some("pal_env_get"),
        "env_set" => Some("pal_env_set"),
        "env_cwd" => Some("pal_env_cwd"),
        "env_all" => Some("pal_env_all"),
        _ => None,
    }
}

fn compile_pal_builtin(
    inst: &MirInst,
    args: &[MirValue],
    pal_name: &str,
    ctx: &mut LlvmCtx,
    extra: &mut Vec<String>,
) -> String {
    let result_ty = match &inst.op {
        MirOp::Call(_, _, ty) => llvm_type_str(&ty.data_type),
        _ => "void".to_string(),
    };
    let expect_bool = result_ty == "i1";
    let pal_ret = if result_ty == "void" { "void" } else { "i64" };
    let result = if pal_ret == "void" {
        None
    } else {
        Some(tmp_result(ctx, pal_ret, inst.result))
    };
    let arg_strs: Vec<String> = args.iter().map(|a| {
        let (v, t) = resolve_typed(a, ctx);
        format!("{} {}", t, v)
    }).collect();
    let call_line = match result {
        Some(r) => format!("%t{} = call {} @{}({})", r, pal_ret, pal_name, arg_strs.join(", ")),
        None => format!("call {} @{}({})", pal_ret, pal_name, arg_strs.join(", ")),
    };
    if expect_bool {
        // PAL returns i64 but mire expects i1; insert icmp ne
        let r = result.unwrap();
        let conv = tmp_extra(ctx, "i1");
        extra.push(call_line);
        extra.push(format!("{} = icmp ne i64 %t{}, 0", conv, r));
        // Re-register the original result id with i1 type for upstream consumers
        if let Some(mir_id) = inst.result {
            ctx.vars.insert(mir_id, conv.clone());
            ctx.temp_types.insert(mir_id, "i1".to_string());
        }
        String::new()
    } else {
        call_line
    }
}

fn compile_inst(inst: &MirInst, ctx: &mut LlvmCtx) -> Vec<String> {
    let mut extra = Vec::new();
    let line = match &inst.op {
        MirOp::Alloca(ty) => {
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, "ptr", inst.result);
            format!("%t{} = alloca {}", result, llty)
        }
        MirOp::Load(src, ty) => {
            let (src_s, _) = resolve_typed(src, ctx);
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &llty, inst.result);
            format!("%t{} = load {}, ptr {}", result, llty, src_s)
        }
        MirOp::Store(dst, src) => {
            let (src_s, src_ty) = resolve_typed(src, ctx);
            let (dst_s, _) = resolve_typed(dst, ctx);
            format!("store {} {}, ptr {}", src_ty, src_s, dst_s)
        }
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
                format!("%t{} = call ptr @rt_string_concat(ptr {}, ptr {})", result, lhs, rhs)
            } else {
                let ty = if lt == "double" || rt == "double" { "double" } else { "i64" };
                let result = tmp_result(ctx, ty, inst.result);
                let op = if ty == "double" { "fadd" } else { "add" };
                let l_final = coerce_to(&l_str, &lt, &ty, ctx, &mut extra);
                let r_final = coerce_to(&r_str, &rt, &ty, ctx, &mut extra);
                format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
            }
        }
        MirOp::Sub(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "double" || rt == "double" { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let op = if ty == "double" { "fsub" } else { "sub" };
            let l_final = coerce_to(&l_str, &lt, &ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, &ty, ctx, &mut extra);
            format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
        }
        MirOp::Mul(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "double" || rt == "double" { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let op = if ty == "double" { "fmul" } else { "mul" };
            let l_final = coerce_to(&l_str, &lt, &ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, &ty, ctx, &mut extra);
            format!("%t{} = {} {} {}, {}", result, op, ty, l_final, r_final)
        }
        MirOp::SDiv(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let is_double = lt == "double" || rt == "double";
            let ty = if is_double { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to(&l_str, &lt, &ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, &ty, ctx, &mut extra);
            if is_double {
                format!("%t{} = fdiv double {}, {}", result, l_final, r_final)
            } else {
                format!("%t{} = call i64 @rt_div_i64(i64 {}, i64 {})", result, l_final, r_final)
            }
        }
        MirOp::SRem(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let is_double = lt == "double" || rt == "double";
            let ty = if is_double { "double" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to(&l_str, &lt, &ty, ctx, &mut extra);
            let r_final = coerce_to(&r_str, &rt, &ty, ctx, &mut extra);
            if is_double {
                format!("%t{} = frem double {}, {}", result, l_final, r_final)
            } else {
                format!("%t{} = call i64 @rt_rem_i64(i64 {}, i64 {})", result, l_final, r_final)
            }
        }
        MirOp::Shl(l, r) => {
            let (l, _lt) = resolve_typed(l, ctx);
            let (r, _) = resolve_typed(r, ctx);
            let result = tmp_result(ctx, "i64", inst.result);
            format!("%t{} = shl i64 {}, {}", result, l, r)
        }
        MirOp::And(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "i1" || rt == "i1" { "i1" } else { "i64" };
            let result = tmp_result(ctx, ty, inst.result);
            let l_final = coerce_to_bool(&l_str, &lt, ctx, &mut extra);
            let r_final = coerce_to_bool(&r_str, &rt, ctx, &mut extra);
            format!("%t{} = and {} {}, {}", result, ty, l_final, r_final)
        }
        MirOp::Or(l, r) => {
            let (l_str, lt) = resolve_typed(l, ctx);
            let (r_str, rt) = resolve_typed(r, ctx);
            let ty = if lt == "i1" || rt == "i1" { "i1" } else { "i64" };
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
                format!("%t{} = fcmp {} {} {}, {}", result, cond, ty, l_final, r_final)
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
                extra.push(format!("{} = call i32 @strcmp(ptr {}, ptr {})", scmp, l_ptr, r_ptr));
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
                let line = match t.as_str() {
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
                    _ => {
                        format!("%t{} = call ptr @rt_i64_to_string(i64 {})", result, v)
                    }
                };
                line
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
                        format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", printf_tmp, select)
                    }
                    "ptr" => {
                        format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", printf_tmp, v)
                    }
                    "double" => {
                        format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_f64, double {})", printf_tmp, v)
                    }
                    _ => {
                        format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_i64, {} {})", printf_tmp, t, v)
                    }
                };
                let result = tmp_result(ctx, "ptr", inst.result);
                extra.push(format!("%t{} = inttoptr i64 0 to ptr", result));
                line
                // result is the ptr returned by dasu (dummy null pointer)
            } else if name_opt == Some("env_args") {
                // env_args() builtin — delegate to runtime
                let result = tmp_result(ctx, "ptr", inst.result);
                let argc = tmp_extra(ctx, "i32");
                let argv = tmp_extra(ctx, "ptr");
                extra.push(format!("{} = load i32, ptr @.argc", argc));
                extra.push(format!("{} = load ptr, ptr @.argv", argv));
                format!("%t{} = call ptr @rt_get_args(i32 {}, ptr {})", result, argc, argv)
            } else if let Some(llvm_name) = builtin_to_pal(name_opt.unwrap_or("")) {
                compile_pal_builtin(inst, args, llvm_name, ctx, &mut extra)
            } else if name_opt == Some("call") {
                // Indirect call via function pointer
                if args.len() < 1 {
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
                    extra.push(format!("{} = inttoptr {} {} to ptr", tmp, fn_ptr_ty, fn_ptr));
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
                            extra.push(format!(
                                "{} = inttoptr {} {} to ptr",
                                tmp, env_ty, env_str
                            ));
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
                extra.push(format!("{} = bitcast ptr {} to {}", fn_cast, fn_ptr_final, fn_sig));
                match result {
                    Some(r) => format!(
                        "%t{} = call {} {}({})",
                        r, ll_ret, fn_cast, arg_strs.join(", ")
                    ),
                    None => format!(
                        "call {} {}({})",
                        ll_ret, fn_cast, arg_strs.join(", ")
                    ),
                }
            } else {
                // Regular function call — prefix user-defined functions with @fn_
                let is_void = matches!(ret_ty.data_type, DataType::None);
                let ll_ret: String = if is_void { "void".to_string() } else { llvm_type_str(&ret_ty.data_type) };
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
                        resolve_named_call(name, env, args, &ll_ret, is_void, inst.result, ctx, &mut extra)
                    }
                    MirValue::Global(name) => {
                        resolve_named_call(name, &MirValue::Const(MirConst::None), args, &ll_ret, is_void, inst.result, ctx, &mut extra)
                    }
                    MirValue::Temp(callee_id) => {
                        // Indirect call through a function value stored in a temp.
                        // The temp is expected to be a pointer to { fn_ptr, env_ptr }.
                        let fn_ptr = tmp_extra(ctx, "ptr");
                        let env_ptr = tmp_extra(ctx, "ptr");
                        let callee_name = ctx.vars.get(callee_id).cloned().unwrap_or_else(|| format!("%t{}", callee_id));
                        extra.push(format!("{} = getelementptr {{ ptr, ptr }}, ptr {}, i32 0, i32 0", fn_ptr, callee_name));
                        extra.push(format!("{} = getelementptr {{ ptr, ptr }}, ptr {}, i32 0, i32 1", env_ptr, callee_name));
                        let fn_ptr_loaded = tmp_extra(ctx, "ptr");
                        let env_ptr_loaded = tmp_extra(ctx, "ptr");
                        extra.push(format!("{} = load ptr, ptr {}", fn_ptr_loaded, fn_ptr));
                        extra.push(format!("{} = load ptr, ptr {}", env_ptr_loaded, env_ptr));
                        let mut param_tys: Vec<String> = vec!["ptr".to_string()];
                        arg_strs.insert(0, format!("ptr {}", env_ptr_loaded));
                        for a in args {
                            let (_, t) = resolve_typed(a, ctx);
                            param_tys.push(t.clone());
                        }
                        let fn_sig = format!("{} ({})*", ll_ret, param_tys.join(", "));
                        let fn_cast = tmp_extra(ctx, "ptr");
                        extra.push(format!("{} = bitcast ptr {} to {}", fn_cast, fn_ptr_loaded, fn_sig));
                        if is_void {
                            format!("call void {}({})", fn_cast, arg_strs.join(", "))
                        } else {
                            let result = tmp_result(ctx, &ll_ret, inst.result);
                            format!("%t{} = call {} {}({})", result, ll_ret, fn_cast, arg_strs.join(", "))
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
            let struct_ty = if struct_name.starts_with("struct:") {
                let name = &struct_name[7..];
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
            let (v, _) = resolve_typed(val, ctx);
            let llty = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &llty, inst.result);
            format!("%t{} = zext i1 {} to {}", result, v, llty)
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
            format!("%t{} = select i1 true, {} {}, {} {}", result, ty, src, ty, src)
        }
        MirOp::Sitofp(val, ty) => {
            let (v, src_t) = resolve_typed(val, ctx);
            let dst_t = llvm_type_str(&ty.data_type);
            let result = tmp_result(ctx, &dst_t, inst.result);
            if src_t == dst_t {
                format!("%t{} = select i1 true, {} {}, {} {}", result, src_t, v, src_t, v)
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
                format!("%t{} = select i1 true, {} {}, {} {}", result, src_t, v, src_t, v)
            } else {
                format!("%t{} = fptosi {} {} to {}", result, src_t, v, dst_t)
            }
        }
        _ => return vec![],
    };
    let mut result = Vec::with_capacity(extra.len() + 1);
    result.extend(extra);
    result.push(line);
    result
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
