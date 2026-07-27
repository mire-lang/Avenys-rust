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
        // Stateless services (no handles, no structs)
        "time.now.ms" | "time_unix_ms" => Some("pal_time_now_ms"),
        "time.now.ns" => Some("pal_time_now_ns"),
        "cpu.count" | "cpu_count" => Some("pal_cpu_count"),
        "mem.total" => Some("pal_mem_total"),
        "mem.available" => Some("pal_mem_available"),
        "mem.process" => Some("pal_mem_process"),
        "random.fill" => Some("pal_random_fill"),
        // Memory (PAL-owned)
        "pal.alloc" => Some("pal_alloc"),
        "pal.free" => Some("pal_free"),
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
        let r = result.unwrap();
        let conv = tmp_extra(ctx, "i1");
        extra.push(call_line);
        extra.push(format!("{} = icmp ne i64 %t{}, 0", conv, r));
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
        // ── Runtime helpers ────────────────────────────────────────────
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
        "declare ptr @ireru(ptr)".to_string(),
        // ── PAL v4 — Filesystem ────────────────────────────────────────
        // Handles are {i32 index, i32 generation} = i64 in LLVM
        "declare i64 @pal_root_open(ptr)".to_string(),
        "declare void @pal_root_close(i64)".to_string(),
        "declare i64 @pal_file_open(i64, ptr, i32)".to_string(),
        "declare i64 @pal_file_read(i64, ptr, i64)".to_string(),
        "declare i64 @pal_file_write(i64, ptr, i64)".to_string(),
        "declare i64 @pal_file_seek(i64, i64, i32)".to_string(),
        "declare i1 @pal_file_stat(i64, ptr)".to_string(),
        "declare i64 @pal_file_size(i64)".to_string(),
        "declare i64 @pal_file_clone(i64)".to_string(),
        "declare void @pal_file_close(i64)".to_string(),
        "declare i64 @pal_dir_open(i64, ptr)".to_string(),
        "declare i1 @pal_dir_next(i64, ptr)".to_string(),
        "declare void @pal_dir_close(i64)".to_string(),
        // ── PAL v4 — Process ───────────────────────────────────────────
        "declare i64 @pal_proc_create(ptr, i32, i64, i64, i64)".to_string(),
        "declare i64 @pal_proc_wait(i64)".to_string(),
        "declare i1 @pal_proc_kill(i64)".to_string(),
        "declare i64 @pal_proc_stdin(i64)".to_string(),
        "declare i64 @pal_proc_stdout(i64)".to_string(),
        "declare i64 @pal_proc_stderr(i64)".to_string(),
        "declare i64 @pal_proc_transfer(i64)".to_string(),
        "declare void @pal_proc_close(i64)".to_string(),
        // ── PAL v4 — Channels ──────────────────────────────────────────
        "declare i64 @pal_channel_create()".to_string(),
        "declare i64 @pal_channel_send(i64, ptr, i64)".to_string(),
        "declare i1 @pal_channel_recv(i64, ptr)".to_string(),
        "declare void @pal_channel_close(i64)".to_string(),
        // ── PAL v4 — Networking ────────────────────────────────────────
        "declare i64 @pal_socket_connect(ptr, i16, i32)".to_string(),
        "declare i64 @pal_listener_bind(i16, i32)".to_string(),
        "declare i64 @pal_listener_accept(i64)".to_string(),
        "declare i64 @pal_socket_send(i64, ptr, i64)".to_string(),
        "declare i64 @pal_socket_recv(i64, ptr, i64)".to_string(),
        "declare void @pal_socket_close(i64)".to_string(),
        "declare void @pal_listener_close(i64)".to_string(),
        // ── PAL v4 — Crypto ────────────────────────────────────────────
        "declare i64 @pal_secret_create(i64)".to_string(),
        "declare i64 @pal_secret_export_public(i64)".to_string(),
        "declare i64 @pal_secret_sign(i64, ptr, i64, ptr, i64)".to_string(),
        "declare i1 @pal_pubkey_verify(i64, ptr, i64, ptr, i64)".to_string(),
        "declare void @pal_secret_close(i64)".to_string(),
        "declare void @pal_pubkey_free(i64)".to_string(),
        // ── PAL v4 — Stateless Services ────────────────────────────────
        "declare i64 @pal_time_now_ms()".to_string(),
        "declare i64 @pal_time_now_ns()".to_string(),
        "declare i64 @pal_cpu_count()".to_string(),
        "declare i64 @pal_mem_total()".to_string(),
        "declare i64 @pal_mem_available()".to_string(),
        "declare i64 @pal_mem_process()".to_string(),
        "declare i1 @pal_random_fill(ptr, i64)".to_string(),
        // ── PAL v4 — Memory ────────────────────────────────────────────
        "declare ptr @pal_alloc(i64)".to_string(),
        "declare void @pal_free(ptr)".to_string(),
        "declare ptr @pal_realloc(ptr, i64)".to_string(),
        "declare ptr @pal_secure_alloc(i64)".to_string(),
        "declare void @pal_secure_free(ptr)".to_string(),
        // ── Safety (spans) ────────────────────────────────────────────
        "declare void @rt_panic_loc(ptr, i64, i64, ptr)".to_string(),
        "declare i64 @rt_div_i64(i64, i64, i64, i64, ptr)".to_string(),
        "declare i64 @rt_rem_i64(i64, i64, i64, i64, ptr)".to_string(),
        "declare void @rt_check_bounds_i64(i64, i64, i64, i64, ptr)".to_string(),
        // ── Maybe[T] ─────────────────────────────────────────────────
        "declare ptr @rt_maybe_some_i64(i64)".to_string(),
        "declare ptr @rt_maybe_some_str(ptr)".to_string(),
        "declare ptr @rt_maybe_some_f64(double)".to_string(),
        "declare ptr @rt_maybe_some_ptr(ptr)".to_string(),
        "declare i64 @rt_maybe_is_none(ptr)".to_string(),
        "declare i64 @rt_maybe_is_some(ptr)".to_string(),
        "declare ptr @rt_maybe_none_as_ptr()".to_string(),
        "declare i64 @rt_maybe_unwrap_i64(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_maybe_unwrap_str(ptr, i64, i64, ptr)".to_string(),
        "declare double @rt_maybe_unwrap_f64(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_maybe_unwrap_ptr(ptr, i64, i64, ptr)".to_string(),
        "declare i64 @rt_maybe_unwrap_or_i64(ptr, i64)".to_string(),
        "declare ptr @rt_maybe_unwrap_or_str(ptr, ptr)".to_string(),
        "declare double @rt_maybe_unwrap_or_f64(ptr, double)".to_string(),
        "declare ptr @rt_maybe_unwrap_or_ptr(ptr, ptr)".to_string(),
        "declare void @rt_maybe_free(ptr)".to_string(),
        // ── Result[T E] ──────────────────────────────────────────────
        "declare ptr @rt_result_ok_i64(i64)".to_string(),
        "declare ptr @rt_result_ok_str(ptr)".to_string(),
        "declare ptr @rt_result_ok_ptr(ptr)".to_string(),
        "declare ptr @rt_result_err_i64(i64)".to_string(),
        "declare ptr @rt_result_err_str(ptr)".to_string(),
        "declare ptr @rt_result_err_ptr(ptr)".to_string(),
        "declare i64 @rt_result_is_ok(ptr)".to_string(),
        "declare i64 @rt_result_is_err(ptr)".to_string(),
        "declare ptr @rt_result_err_payload(ptr)".to_string(),
        "declare i64 @rt_result_unwrap_i64(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_result_unwrap_str(ptr, i64, i64, ptr)".to_string(),
        "declare double @rt_result_unwrap_f64(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_result_unwrap_ptr(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_result_unwrap_err_str(ptr, i64, i64, ptr)".to_string(),
        "declare i64 @rt_result_unwrap_or_i64(ptr, i64)".to_string(),
        "declare ptr @rt_result_unwrap_or_str(ptr, ptr)".to_string(),
        "declare double @rt_result_unwrap_or_f64(ptr, double)".to_string(),
        "declare ptr @rt_result_unwrap_or_ptr(ptr, ptr)".to_string(),
        "declare void @rt_result_free(ptr)".to_string(),
        // ── Arr[T N] ─────────────────────────────────────────────────
        "declare i64 @rt_arr_len(ptr, i64)".to_string(),
        "declare i64 @rt_arr_first_i64(ptr, i64, i64, i64, ptr)".to_string(),
        "declare i64 @rt_arr_last_i64(ptr, i64, i64, i64, ptr)".to_string(),
        "declare i64 @rt_arr_contains_i64(ptr, i64, i64)".to_string(),
        "declare i64 @rt_arr_index_of_i64(ptr, i64, i64)".to_string(),
        "declare void @rt_arr_reverse_i64(ptr, i64)".to_string(),
        "declare ptr @rt_arr_join(ptr, i64, ptr)".to_string(),
        // ── Lists with spans ─────────────────────────────────────────
        "declare i64 @rt_lists_first(ptr, i64, i64, ptr)".to_string(),
        "declare i64 @rt_lists_last(ptr, i64, i64, ptr)".to_string(),
        // ── Dicts ────────────────────────────────────────────────────
        "declare ptr @rt_dict_ensure(ptr)".to_string(),
        "declare i64 @rt_dict_len(ptr)".to_string(),
        "declare i64 @rt_dict_get_i64(ptr, i64, i64, ptr, i64)".to_string(),
        "declare ptr @rt_dict_set_i64(ptr, i64, i64, i64, ptr, i64)".to_string(),
        "declare ptr @rt_dict_get_ptr(ptr, i64, i64, ptr, ptr)".to_string(),
        "declare ptr @rt_dict_set_ptr(ptr, i64, i64, i64, ptr, ptr)".to_string(),
        "declare i64 @rt_dict_has(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_dict_remove(ptr, i64, i64, ptr)".to_string(),
        "declare ptr @rt_dict_to_string(ptr)".to_string(),
        "declare void @rt_dict_free(ptr)".to_string(),
        "declare ptr @rt_dict_keys(ptr)".to_string(),
        "declare ptr @rt_dict_values(ptr)".to_string(),
        "declare ptr @rt_dicts_set(ptr, ptr, ptr)".to_string(),
        "declare ptr @rt_dicts_get(ptr, ptr)".to_string(),
    ]
}
