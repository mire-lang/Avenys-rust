use super::resolve::resolve_typed;
use super::types::llvm_type_str;
use super::{LlvmCtx, tmp_extra, tmp_result};
use crate::compiler::mir::{MirInst, MirOp, MirValue};

/// Maps a builtin function name to its PAL/LLVM callee name.
pub(crate) fn builtin_to_pal(name: &str) -> Option<&'static str> {
    match name {
        // Math
        "abs" => Some("abs"),
        "sqrt" => Some("rt_math_sqrt"),
        "pow" => Some("rt_math_pow"),
        "round" => Some("rt_math_round"),
        "floor" => Some("rt_math_floor"),
        "ceil" => Some("rt_math_ceil"),
        // Filesystem
        "fs.write" | "fs_write" => Some("pal_fs_write"),
        "fs.append" => Some("pal_fs_append"),
        "fs.read" | "fs_read" => Some("pal_fs_read"),
        "fs.copy" => Some("pal_fs_copy"),
        "fs.move" => Some("pal_fs_move"),
        "fs.drop" | "fs_drop" => Some("pal_fs_delete"),
        "fs.mkdir" => Some("pal_fs_mkdir"),
        "fs.rmdir" => Some("pal_fs_rmdir"),
        "fs.exists" | "fs_exists" => Some("pal_fs_exists"),
        "fs.is_dir" | "fs_is_dir" => Some("pal_fs_is_dir"),
        "fs.is_file" | "fs_is_file" => Some("pal_fs_is_file"),
        "fs.size" => Some("pal_fs_size"),
        "sleep_ms" => Some("pal_time_sleep_ms"),
        "fs.list" | "fs_list" => Some("pal_fs_list"),
        "fs.walk" => Some("pal_fs_walk"),
        "fs.join" => Some("pal_fs_join"),
        "fs.dir" | "fs_dir" => Some("pal_fs_dir"),
        "fs.name" | "fs_name" => Some("pal_fs_name"),
        "fs.ext" | "fs_ext" => Some("pal_fs_ext"),
        "fs.write_secure" => Some("pal_fs_write_secure"),
        "fs.chmod" => Some("pal_fs_chmod"),
        // Process
        "proc.run" | "proc_run" => Some("pal_proc_run"),
        "proc.exec" | "proc_exec" => Some("pal_proc_exec"),
        "proc.spawn" | "proc_spawn" => Some("pal_proc_spawn"),
        "proc.wait" | "proc_wait" => Some("pal_proc_wait"),
        "proc.kill" => Some("pal_proc_kill"),
        "proc.exit" | "proc_exit" => Some("pal_proc_exit"),
        "proc.exists" | "proc_exists" => Some("pal_proc_exists"),
        "proc.on" => Some("pal_proc_on"),
        "proc.spawn_argv" => Some("pal_proc_spawn_argv"),
        "proc.last_signal" => Some("pal_proc_last_signal"),
        // Environment
        "env.get" | "env_get" => Some("pal_env_get"),
        "env.set" => Some("pal_env_set"),
        "env.cwd" | "env_cwd" => Some("pal_env_cwd"),
        "env.all" | "env_all" => Some("pal_env_all"),
        // Time
        "time.now.ms" | "time_unix_ms" => Some("pal_time_unix_ms"),
        "time.now.ns" => Some("pal_time_unix_ns"),
        // Memory
        "mem.used" | "mem_used" => Some("pal_mem_used"),
        // CPU
        "cpu.count" | "cpu_count" => Some("pal_cpu_count"),
        // Networking
        "net.bind" => Some("pal_net_bind"),
        "net.accept" => Some("pal_net_accept"),
        // Threads
        "thread.detach" => Some("pal_thread_detach"),
        _ => None,
    }
}

pub(crate) fn compile_pal_builtin(
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
    let arg_strs: Vec<String> = args
        .iter()
        .map(|a| {
            let (v, t) = resolve_typed(a, ctx);
            format!("{} {}", t, v)
        })
        .collect();
    let call_line = match result {
        Some(r) => format!(
            "%t{} = call {} @{}({})",
            r,
            pal_ret,
            pal_name,
            arg_strs.join(", ")
        ),
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

pub(crate) fn pal_extern_decls() -> Vec<String> {
    vec![
        "declare ptr @rt_get_args(i32, ptr)".to_string(),
        "declare ptr @rt_bool_to_string(i64)".to_string(),
        "declare ptr @rt_managed_from_cstr(ptr)".to_string(),
        "declare ptr @rt_managed_ensure_managed(ptr)".to_string(),
        "declare void @rt_managed_free(ptr)".to_string(),
        "declare void @free(ptr)".to_string(),
        "declare ptr @malloc(i64)".to_string(),
        "declare ptr @rt_f64_to_string(double)".to_string(),
        "declare ptr @rt_f32_to_string(float)".to_string(),
        "declare ptr @rt_i64_to_string(i64)".to_string(),
        "declare ptr @rt_i128_to_string(i128)".to_string(),
        "declare ptr @rt_u128_to_string(i128)".to_string(),
        "declare i32 @fflush(ptr)".to_string(),
        "declare i64 @abs(i64)".to_string(),
        "declare double @rt_math_sqrt(double)".to_string(),
        "declare double @rt_math_pow(double, double)".to_string(),
        "declare i64 @rt_math_round(double)".to_string(),
        "declare i64 @rt_math_floor(double)".to_string(),
        "declare i64 @rt_math_ceil(double)".to_string(),
        "declare i64 @pal_fs_delete(ptr)".to_string(),
        "declare ptr @ireru(ptr)".to_string(),
        "declare void @pal_time_sleep_ms(i64)".to_string(),
        "declare i64 @rt_dict_get_i64(ptr, i64, i64, ptr, i64)".to_string(),
        "declare ptr @rt_dict_get_ptr(ptr, i64, i64, ptr, ptr)".to_string(),
        "declare ptr @rt_dict_set_i64(ptr, i64, i64, i64, ptr, i64)".to_string(),
        "declare ptr @rt_dict_set_ptr(ptr, i64, i64, i64, ptr, ptr)".to_string(),
        "declare i64 @rt_thread_spawn_closure(ptr, ptr)".to_string(),
        "declare i64 @pal_fs_write(ptr, ptr)".to_string(),
        "declare i64 @pal_fs_append(ptr, ptr)".to_string(),
        "declare ptr @pal_fs_read(ptr)".to_string(),
        "declare i64 @pal_fs_copy(ptr, ptr)".to_string(),
        "declare i64 @pal_fs_move(ptr, ptr)".to_string(),
        "declare i64 @pal_fs_mkdir(ptr)".to_string(),
        "declare i64 @pal_fs_rmdir(ptr)".to_string(),
        "declare i64 @pal_fs_exists(ptr)".to_string(),
        "declare i64 @pal_fs_is_dir(ptr)".to_string(),
        "declare i64 @pal_fs_is_file(ptr)".to_string(),
        "declare i64 @pal_fs_size(ptr)".to_string(),
        "declare ptr @pal_fs_list(ptr)".to_string(),
        "declare ptr @pal_fs_walk(ptr)".to_string(),
        "declare ptr @pal_fs_join(ptr, ptr)".to_string(),
        "declare ptr @pal_fs_dir(ptr)".to_string(),
        "declare ptr @pal_fs_name(ptr)".to_string(),
        "declare ptr @pal_fs_ext(ptr)".to_string(),
        "declare ptr @pal_proc_run(ptr)".to_string(),
        "declare ptr @pal_proc_exec(ptr)".to_string(),
        "declare i64 @pal_proc_spawn(ptr)".to_string(),
        "declare i64 @pal_proc_wait(i64)".to_string(),
        "declare i64 @pal_proc_kill(i64)".to_string(),
        "declare void @pal_proc_exit(i64)".to_string(),
        "declare i64 @pal_proc_exists(i64)".to_string(),
        "declare void @pal_proc_on(ptr)".to_string(),
        "declare ptr @pal_env_get(ptr)".to_string(),
        "declare i32 @pal_env_set(ptr, ptr)".to_string(),
        "declare ptr @pal_env_cwd()".to_string(),
        "declare ptr @pal_env_all()".to_string(),
        "declare i64 @pal_thread_join(i64, ptr)".to_string(),
        "declare i64 @pal_time_unix_ms()".to_string(),
        "declare i64 @pal_time_unix_ns()".to_string(),
        "declare i64 @pal_mem_used()".to_string(),
        "declare i64 @pal_cpu_count()".to_string(),
        "declare i64 @pal_proc_spawn_argv(ptr)".to_string(),
        "declare i64 @pal_proc_last_signal()".to_string(),
        "declare i64 @pal_net_bind(i64)".to_string(),
        "declare i64 @pal_net_accept(i64)".to_string(),
        "declare void @pal_thread_detach(i64)".to_string(),
        "declare i64 @pal_fs_write_secure(ptr, ptr, i32)".to_string(),
        "declare i64 @pal_fs_chmod(ptr, i32)".to_string(),
    ]
}
