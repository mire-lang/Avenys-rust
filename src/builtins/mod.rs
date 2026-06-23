use std::collections::HashMap;

use crate::parser::ast::DataType;

pub fn default_builtin_returns() -> HashMap<String, DataType> {
    let mut builtins = HashMap::new();

    for name in [
        "dasu",
        "push",
        "append",
        "remove",
        "time_sleep_ms",
        "time_sleep_ns",
        "fs_write",
        "fs_append",
        "fs_copy",
        "fs_move",
        "fs_drop",
        "fs_mkdir",
        "fs_rmdir",
        "env_set",
        "env_chdir",
        "proc_kill",
        "proc_write",
        "proc_on",
        "proc_exit",
    ] {
        builtins.insert(name.to_string(), DataType::None);
    }

    for name in [
        "len",
        "time_now_ms",
        "time_now_ns",
        "time_since_ms",
        "time_since_ns",
        "time_mark",
        "time_elapsed_ms",
        "time_elapsed_ns",
        "time.mark",
        "time.elapsed_ns",
        "mem_used",
        "mem_total",
        "mem_free",
        "mem_available",
        "mem_process",
        "mem.process",
        "cpu_time_ns",
        "cpu_time_ms",
        "cpu_mark",
        "cpu_elapsed_ns",
        "cpu_count",
        "cpu_cycles_est",
        "cpu.cycles_est",
        "cpu.mark",
        "sum",
        "min",
        "max",
        "abs",
        "round",
        "floor",
        "ceil",
        "clamp",
        "fs_size",
        "proc_wait",
        "lists.len",
        "lists.get",
        "strings.len",
    ] {
        builtins.insert(name.to_string(), DataType::I64);
    }

    for name in ["lists.push", "lists.set", "lists.slice"] {
        builtins.insert(
            name.to_string(),
            DataType::Vector {
                element_type: Box::new(DataType::Anything),
                dynamic: true,
            },
        );
    }

    for name in ["lists.fold", "lists.map", "lists.filter"] {
        builtins.insert(name.to_string(), DataType::Unknown);
    }

    for name in [
        "strings.replace",
        "strings.join",
        "strings.to_upper",
        "strings.to_lower",
        "strings.trim",
        "strings.concat",
        "strings.to_string",
        "strings.replace_first",
        "mem.format",
        "gpu.snapshot",
        "time.elapsed_ms",
        "cpu.elapsed_ms",
        "cpu_elapsed_ms",
    ] {
        builtins.insert(name.to_string(), DataType::Str);
    }

    builtins.insert(
        "strings.split".to_string(),
        DataType::Vector {
            element_type: Box::new(DataType::Str),
            dynamic: true,
        },
    );

    for name in [
        "ireru",
        "__mire_fmt",
        "mem_format_bytes",
        "fs_read",
        "fs_join",
        "fs_dir",
        "fs_name",
        "fs_ext",
        "env_get",
        "env_cwd",
        "proc_run",
        "proc_exec",
        "proc_shell",
        "proc_exec_pipe",
        "proc_pipe",
        "proc_read",
        "strings.to_upper",
        "strings.to_lower",
        "strings.trim",
        "strings.concat",
    ] {
        builtins.insert(name.to_string(), DataType::Str);
    }

    for name in [
        "fs_exists",
        "fs_is_dir",
        "fs_is_file",
        "proc_exists",
        "gpu_available",
        "strings.starts_with",
        "strings.ends_with",
    ] {
        builtins.insert(name.to_string(), DataType::Bool);
    }

    for name in ["lists.keys", "lists.values", "lists.slice", "range"] {
        builtins.insert(
            name.to_string(),
            DataType::Vector {
                element_type: Box::new(DataType::Anything),
                dynamic: true,
            },
        );
    }

    builtins.insert(
        "fs_list".to_string(),
        DataType::Vector {
            element_type: Box::new(DataType::Str),
            dynamic: true,
        },
    );

    builtins.insert(
        "env_args".to_string(),
        DataType::Vector {
            element_type: Box::new(DataType::Str),
            dynamic: true,
        },
    );

    for name in [
        "env_all",
        "mem_snapshot",
        "mem.snapshot",
        "cpu_loadavg",
        "cpu_snapshot",
        "cpu.snapshot",
        "gpu_snapshot",
        "dicts.set",
        "dicts.keys",
        "dicts.values",
        "dicts.to_string",
    ] {
        builtins.insert(
            name.to_string(),
            DataType::Map {
                key_type: Box::new(DataType::Anything),
                value_type: Box::new(DataType::Anything),
            },
        );
    }
    builtins.insert("dicts.get".to_string(), DataType::Anything);

    for name in [
        "int",
        "float",
        "bool",
        "type",
        "sort",
        "reverse",
        "unique",
        "trim",
        "ltrim",
        "rtrim",
        "substr",
        "pad_left",
        "pad_right",
        "first",
        "last",
        "slice",
        "concat",
        "flatten",
        "is_int",
        "is_float",
        "is_bool",
        "is_str",
        "is_list",
        "is_dict",
        "is_none",
        "contains",
        "index_of",
        "ram_usage",
        "mem_percent",
        "cpu_freq_mhz",
        "proc_spawn",
        "proc_exec_bg",
    ] {
        builtins.insert(name.to_string(), DataType::Anything);
    }

    builtins.insert("str".to_string(), DataType::Str);
    builtins.insert("range".to_string(), DataType::List);
    builtins.insert("call".to_string(), DataType::Unknown);
    builtins.insert("__if_expr".to_string(), DataType::Unknown);
    builtins.insert("__do_while".to_string(), DataType::None);
    builtins.insert("__type_matches".to_string(), DataType::Bool);
    builtins.insert("__is".to_string(), DataType::Bool);
    builtins.insert("new::".to_string(), DataType::Unknown);
    builtins.insert("own::".to_string(), DataType::Box);
    builtins.insert("move::".to_string(), DataType::Unknown);
    builtins.insert("drop::".to_string(), DataType::None);

    builtins
}

pub fn std_module_members(module: &str) -> &'static [&'static str] {
    match module {
        "math" => &[
            "abs",
            "min",
            "max",
            "sum",
            "mean",
            "avg",
            "variance",
            "stddev",
            "median",
            "min_list",
            "max_list",
            "range",
            "range_between",
            "range_step",
            "round",
            "floor",
            "ceil",
            "clamp",
            "pi",
            "e",
            "tau",
            "sin",
            "cos",
            "tan",
            "sqrt",
            "pow",
            "log",
            "log10",
            "exp",
            "atan2",
            "asin",
            "acos",
        ],
        "strings" => &[
            "upper",
            "lower",
            "strip",
            "split",
            "replace",
            "contains",
            "startswith",
            "endswith",
            "len",
            "trim",
            "ltrim",
            "rtrim",
            "substr",
            "pad_left",
            "pad_right",
            "repeat",
            "is_empty",
        ],
        "lists" => &[
            "len", "push", "pop", "remove", "delete", "append", "clear", "join", "contains",
            "index_of", "first", "last", "slice", "concat", "flatten", "reverse", "sort", "unique",
            "is_empty",
        ],
        "dicts" => &[
            "len", "keys", "values", "has", "get", "set", "remove", "delete", "entries", "merge",
            "is_empty",
        ],
        "time" => &[
            "unix_ms",
            "unix_ns",
            "since_ms",
            "since_ns",
            "mark",
            "elapsed_ms",
            "elapsed_ns",
            "sleep_ms",
            "sleep_ns",
        ],
        "term" => &["style", "hr", "clear"],
        "mem" => &[
            "used",
            "total",
            "free",
            "available",
            "percent",
            "process",
            "snapshot",
            "format",
        ],
        "cpu" => &[
            "time_ns",
            "time_ms",
            "mark",
            "elapsed_ns",
            "elapsed_ms",
            "count",
            "freq_mhz",
            "cycles_est",
            "loadavg",
            "snapshot",
        ],
        "gpu" => &["available", "snapshot"],
        "net" => &["connect", "send", "recv", "close", "poll", "resolve", "http_get", "http_post", "recv_all", "set_nonblock", "connect_timeout"],
        "fs" => &[
            "read", "write", "append", "exists", "size", "copy", "move", "drop", "list", "mkdir",
            "rmdir", "join", "dir", "name", "ext", "is_file", "walk",
        ],
        "env" => &["get", "set", "all", "args", "cwd", "chdir"],
        "proc" => &[
            "run", "spawn", "pipe", "shell", "read", "write", "on", "exit", "err", "exec",
            "exec_bg", "kill", "wait", "exists",
        ],
        _ => &[],
    }
}
