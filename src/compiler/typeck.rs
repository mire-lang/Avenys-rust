use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::error::{MireError, Result};
use crate::lexer::tokenize;
use crate::parser::Parser;
use crate::parser::ast::{
    DataType, Expression, Identifier, Literal, MireValue, Program, QueryOp, Statement,
    TraitMethodSig,
};

#[derive(Debug, Clone)]
struct FunctionSig {
    params: Vec<DataType>,
    return_type: DataType,
}

#[derive(Debug, Clone)]
struct ClassFieldSig {
    name: String,
    data_type: DataType,
    has_default: bool,
}

#[derive(Debug, Clone)]
struct ClassSig {
    fields: Vec<ClassFieldSig>,
}

#[derive(Debug, Clone)]
struct EnumVariantSig {
    payload_types: Vec<DataType>,
}

#[derive(Debug, Clone)]
struct TraitSig {
    methods: Vec<TraitMethodSig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    Instance,
    Associated,
}

pub fn check_program_types(program: &mut Program, source: &str) -> Result<()> {
    TYPE_CHECKER_SOURCE.with(|s| {
        *s.borrow_mut() = Some(source.to_string());
    });

    let mut checker = TypeChecker::new();
    checker.collect_function_signatures(&program.statements)?;
    checker.check_top_level_statements(&mut program.statements)
}

pub fn check_program_types_with_origins(
    program: &mut Program,
    source: &str,
    statement_origins: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
) -> Result<()> {
    TYPE_CHECKER_SOURCE.with(|s| {
        *s.borrow_mut() = Some(source.to_string());
    });

    let mut checker = TypeChecker::new();
    checker.statement_origins = statement_origins
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    checker.sources_by_filename = sources
        .iter()
        .map(|(path, source)| (path.display().to_string(), source.clone()))
        .collect();
    checker.collect_function_signatures(&program.statements)?;
    checker.check_top_level_statements(&mut program.statements)
}

thread_local! {
    static TYPE_CHECKER_SOURCE: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
}

struct TypeChecker {
    scopes: Vec<HashMap<String, (DataType, bool)>>,
    struct_scopes: Vec<HashMap<String, String>>,
    functions: HashMap<String, FunctionSig>,
    classes: HashMap<String, ClassSig>,
    enum_variants: HashMap<String, EnumVariantSig>,
    traits: HashMap<String, TraitSig>,
    builtin_returns: HashMap<String, DataType>,
    return_type_stack: Vec<DataType>,
    visited_libs: HashSet<String>,
    impl_self_type: Option<DataType>,
    impl_self_name: Option<String>,
    statement_origins: Vec<String>,
    sources_by_filename: HashMap<String, String>,
    current_filename: Option<String>,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            struct_scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            classes: HashMap::new(),
            enum_variants: HashMap::new(),
            traits: HashMap::new(),
            builtin_returns: Self::default_builtin_returns(),
            return_type_stack: Vec::new(),
            visited_libs: HashSet::new(),
            impl_self_type: None,
            impl_self_name: None,
            statement_origins: Vec::new(),
            sources_by_filename: HashMap::new(),
            current_filename: None,
        }
    }

    fn check_top_level_statements(&mut self, statements: &mut [Statement]) -> Result<()> {
        for (index, statement) in statements.iter_mut().enumerate() {
            self.current_filename = self.statement_origins.get(index).cloned();
            self.check_statement(statement)
                .map_err(|err| self.attach_current_context(err))?;
        }
        Ok(())
    }

    fn attach_current_context(&self, err: MireError) -> MireError {
        let err = if err.filename.is_none() {
            if let Some(filename) = &self.current_filename {
                err.with_filename(filename.clone())
            } else {
                err
            }
        } else {
            err
        };

        if err.source.is_none() {
            if let Some(filename) = err.filename.as_ref() {
                if let Some(source) = self.sources_by_filename.get(filename) {
                    return err.with_source(source.clone());
                }
            }
        }

        err
    }

    fn default_builtin_returns() -> HashMap<String, DataType> {
        let mut builtins = HashMap::new();

        // ── Builtins that return None (side-effect only) ──────────────────────
        for name in [
            // Terminal / output
            "print",
            "println",
            "print_fmt",
            "output",
            "hr",
            "clear",
            "style",
            "dasu",
            // Collections (mutate in-place semantics)
            "push",
            "append",
            "remove",
            // Time
            "time_sleep_ms",
            "time_sleep_ns",
            // Fs – write-side operations
            "fs_write",
            "fs_append",
            "fs_copy",
            "fs_move",
            "fs_drop",
            "fs_mkdir",
            "fs_rmdir",
            // Env – setter operations
            "env_set",
            "env_chdir",
            // Proc – side effects on process table
            "proc_kill",
            "proc_write",
            "proc_on",
            "proc_exit",
        ] {
            builtins.insert(name.to_string(), DataType::None);
        }

        // ── Builtins that return i64 ──────────────────────────────────────────
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
            "math.sum",
            "lists.len",
            "lists.get",
            "strings.len",
        ] {
            builtins.insert(name.to_string(), DataType::I64);
        }

        // Builtins that return list
        for name in ["lists.push", "lists.set", "lists.slice"] {
            builtins.insert(
                name.to_string(),
                DataType::Vector {
                    element_type: Box::new(DataType::Anything),
                    dynamic: true,
                },
            );
        }

        // Builtins that return str
        for name in [
            "strings.replace",
            "strings.split",
            "strings.join",
            "strings.to_upper",
            "strings.to_lower",
            "strings.trim",
            "strings.concat",
            "strings.to_string",
            "mem.format",
            "gpu.snapshot",
            "time.elapsed_ms",
            "cpu.elapsed_ms",
            "cpu_elapsed_ms",
        ] {
            builtins.insert(name.to_string(), DataType::Str);
        }

        // ── Builtins that return str ──────────────────────────────────────────
        for name in [
            "input",
            "ireru",
            "__mire_fmt",
            "mem_format_bytes",
            // Fs content + path helpers
            "fs_read",
            "fs_join",
            "fs_dir",
            "fs_name",
            "fs_ext",
            // Env context
            "env_get",
            "env_cwd",
            // Proc output helpers
            "proc_shell",
            "proc_exec_pipe",
            "proc_pipe",
            "proc_read",
            // String builtins
            "strings.to_upper",
            "strings.to_lower",
            "strings.trim",
            "strings.concat",
        ] {
            builtins.insert(name.to_string(), DataType::Str);
        }

        // ── Builtins that return bool ─────────────────────────────────────────
        for name in ["fs_exists", "proc_exists", "gpu_available"] {
            builtins.insert(name.to_string(), DataType::Bool);
        }

        // ── Builtins that return list ─────────────────────────────────────────
        for name in [
            "fs_list",
            "env_args",
            "lists.keys",
            "lists.values",
            "lists.slice",
            "range",
        ] {
            builtins.insert(
                name.to_string(),
                DataType::Vector {
                    element_type: Box::new(DataType::Anything),
                    dynamic: true,
                },
            );
        }

        // ── Builtins that return dict ─────────────────────────────────────────
        for name in [
            "env_all",
            "mem_snapshot",
            "mem.snapshot",
            "cpu_loadavg",
            "cpu_snapshot",
            "cpu.snapshot",
            "gpu_snapshot",
            "proc_exec",
            "proc_run",
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

        // ── Polymorphic / Anything builtins ───────────────────────────────────
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

        builtins
    }

    fn import_std_members(&mut self, module: &str) {
        let members: &[&str] = match module {
            "math" => &[
                "abs", "min", "max", "sum", "range", "round", "floor", "ceil", "clamp",
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
                "index_of", "first", "last", "slice", "concat", "flatten", "reverse", "sort",
                "unique", "is_empty",
            ],
            "dicts" => &[
                "len", "keys", "values", "has", "get", "set", "remove", "delete", "entries",
                "merge", "is_empty",
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
            "term" => &["print", "println", "style", "hr", "clear", "input"],
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
            "fs" => &[
                "read", "write", "append", "exists", "size", "copy", "move", "drop", "list",
                "mkdir", "rmdir", "join", "dir", "name", "ext",
            ],
            "env" => &["get", "set", "all", "args", "cwd", "chdir"],
            "proc" => &[
                "run", "spawn", "pipe", "shell", "read", "write", "on", "exit", "err", "exec",
                "exec_bg", "kill", "wait", "exists",
            ],
            _ => &[],
        };

        for member in members {
            self.insert_var((*member).to_string(), DataType::Anything, true);
        }
    }

    fn collect_function_signatures(&mut self, statements: &[Statement]) -> Result<()> {
        for statement in statements {
            match statement {
                Statement::Function {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    self.functions.insert(
                        name.clone(),
                        FunctionSig {
                            params: params.iter().map(|(_, t)| t.clone()).collect(),
                            return_type: return_type.clone(),
                        },
                    );
                }
                Statement::Impl {
                    type_name, methods, ..
                } => {
                    for method in methods {
                        if let Statement::Function {
                            name,
                            params,
                            return_type,
                            ..
                        } = method
                        {
                            let mut full_params = params.clone();
                            if let Some((_, self_ty)) =
                                full_params.iter_mut().find(|(param, _)| param == "self")
                            {
                                *self_ty = DataType::StructNamed(type_name.clone());
                            }
                            self.functions.insert(
                                format!("{}.{}", type_name, name),
                                FunctionSig {
                                    params: full_params.iter().map(|(_, t)| t.clone()).collect(),
                                    return_type: return_type.clone(),
                                },
                            );
                        }
                    }
                    self.collect_function_signatures(methods)?;
                }
                Statement::Module { body, .. } => self.collect_function_signatures(body)?,
                Statement::Skill { name, methods, .. } => {
                    self.traits.insert(
                        name.clone(),
                        TraitSig {
                            methods: methods.clone(),
                        },
                    );
                }
                Statement::Trait { name, methods } => {
                    self.traits.insert(
                        name.clone(),
                        TraitSig {
                            methods: methods.clone(),
                        },
                    );
                }
                Statement::Class { name, methods, .. } => {
                    let fields = methods
                        .iter()
                        .filter_map(|statement| match statement {
                            Statement::Let {
                                name,
                                data_type,
                                value,
                                ..
                            } => Some(ClassFieldSig {
                                name: name.clone(),
                                data_type: data_type.clone(),
                                has_default: value.is_some(),
                            }),
                            _ => None,
                        })
                        .collect();
                    self.classes.insert(name.clone(), ClassSig { fields });
                    self.collect_function_signatures(methods)?
                }
                Statement::Type { name, fields, .. } => {
                    let type_fields = fields
                        .iter()
                        .filter_map(|statement| match statement {
                            Statement::Let {
                                name,
                                data_type,
                                value,
                                ..
                            } => Some(ClassFieldSig {
                                name: name.clone(),
                                data_type: data_type.clone(),
                                has_default: value.is_some(),
                            }),
                            _ => None,
                        })
                        .collect();
                    self.classes.insert(
                        name.clone(),
                        ClassSig {
                            fields: type_fields,
                        },
                    );
                    self.collect_function_signatures(fields)?
                }
                Statement::Code { .. } => {
                    // Code no longer supported
                }
                Statement::AddLib { path } => self.collect_library_signatures(path)?,
                Statement::Enum { name, variants, .. } => {
                    for variant in variants {
                        let full_name = format!("{}.{}", name, variant.name);
                        self.enum_variants.insert(
                            full_name,
                            EnumVariantSig {
                                payload_types: variant.data_types.clone(),
                            },
                        );
                    }
                    self.insert_var(name.clone(), DataType::EnumNamed(name.clone()), true);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_library_signatures(&mut self, path: &str) -> Result<()> {
        if !self.visited_libs.insert(path.to_string()) {
            return Ok(());
        }

        let source = fs::read_to_string(path)
            .map_err(|err| type_error(format!("Failed to read library '{}': {}", path, err)))?;
        let tokens = tokenize(&source).map_err(|err| {
            err.with_source(source.clone())
                .with_filename(path.to_string())
        })?;
        let mut parser = Parser::new(tokens);
        let imported = parser
            .parse()
            .map_err(|err| err.with_source(source).with_filename(path.to_string()))?;
        self.collect_function_signatures(&imported.statements)
    }

    fn check_statements(&mut self, statements: &mut [Statement]) -> Result<()> {
        for statement in statements {
            self.check_statement(statement)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &mut Statement) -> Result<()> {
        match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_constant: _,
                is_mutable,
                is_static: _,
                visibility: _,
            } => {
                if let Some(expr) = value {
                    if let Expression::Literal(Literal::Int(int_val)) = expr {
                        Self::validate_int_literal_range(data_type, *int_val)?;
                    }
                }
                let inferred = if let Some(expr) = value {
                    self.check_expression(expr)?
                } else {
                    DataType::Unknown
                };

                let final_type = if *data_type == DataType::Unknown {
                    inferred
                } else {
                    if inferred != DataType::Unknown && !self.is_assignable(data_type, &inferred) {
                        return Err(type_error(format!(
                            "Type mismatch in let '{}': expected {:?}, got {:?}",
                            name, data_type, inferred
                        )));
                    }
                    if let Some(expr) = value.as_ref() {
                        Self::validate_explicit_nested_literal(data_type, expr)?;
                    }
                    data_type.clone()
                };

                *data_type = final_type.clone();
                let mutable = *is_mutable;
                self.insert_var(name.clone(), final_type, mutable);
                self.bind_struct_name(name, value.as_ref());
            }
            Statement::Assignment { target, value, .. } => {
                let value_type = self.check_expression(value)?;
                let (mut target_type, is_target_mutable) =
                    self.lookup_var(target).ok_or_else(|| {
                        type_error(format!("Assignment to undefined variable '{}'", target))
                    })?;

                if !self.is_assignable(&target_type, &value_type) {
                    return Err(type_error(format!(
                        "Type mismatch in assignment to '{}': expected {:?}, got {:?}",
                        target, target_type, value_type
                    )));
                }

                if !is_target_mutable {
                    return Err(type_error(format!(
                        "Cannot reassign immutable variable '{}'",
                        target
                    )));
                }
                Self::validate_explicit_nested_literal(&target_type, value)?;

                target_type = Self::unify_types(&target_type, &value_type)?;
                self.insert_var(target.clone(), target_type, is_target_mutable);
                self.bind_struct_name(target, Some(value));
            }
            Statement::Function {
                name,
                params,
                body,
                return_type,
                ..
            } => {
                self.functions.insert(
                    name.clone(),
                    FunctionSig {
                        params: params.iter().map(|(_, t)| t.clone()).collect(),
                        return_type: return_type.clone(),
                    },
                );

                self.push_scope();
                for (param_name, param_type) in params.iter() {
                    self.insert_var(param_name.clone(), param_type.clone(), true);
                }

                self.return_type_stack.push(return_type.clone());
                self.check_statements(body)?;
                let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);

                if *return_type == DataType::Unknown {
                    *return_type = inferred_return.clone();
                } else if inferred_return != DataType::Unknown
                    && !self.is_assignable(return_type, &inferred_return)
                {
                    return Err(type_error(format!(
                        "Function '{}' return type mismatch: declared {:?}, inferred {:?}",
                        name, return_type, inferred_return
                    )));
                }

                self.pop_scope();

                if let Some(sig) = self.functions.get_mut(name) {
                    sig.return_type = return_type.clone();
                }
            }
            Statement::Return(expr) => {
                let return_type = if let Some(expression) = expr {
                    self.check_expression(expression)?
                } else {
                    DataType::None
                };

                if let Some(current) = self.return_type_stack.last_mut() {
                    let unified = Self::unify_types(current, &return_type)?;
                    *current = unified;
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error(format!(
                        "If condition must be bool, got {:?}",
                        cond_type
                    )));
                }

                self.push_scope();
                self.check_statements(then_branch)?;
                self.pop_scope();

                if let Some(branch) = else_branch {
                    self.push_scope();
                    self.check_statements(branch)?;
                    self.pop_scope();
                }
            }
            Statement::While { condition, body } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error(format!(
                        "While condition must be bool, got {:?}",
                        cond_type
                    )));
                }

                self.push_scope();
                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::For {
                variable,
                iterable,
                body,
            }
            | Statement::Find {
                variable,
                iterable,
                body,
            } => {
                let iter_type = self.check_expression(iterable)?;
                self.push_scope();

                let item_type = match iterable {
                    Expression::Call { name, .. } if name == "range" => DataType::I64,
                    _ => match iter_type {
                        DataType::Array { element_type, .. } | DataType::Slice { element_type } => {
                            *element_type
                        }
                        DataType::Tuple => DataType::Anything,
                        DataType::List => DataType::Anything,
                        DataType::Vector { element_type, .. } => *element_type,
                        DataType::Str => DataType::Str,
                        _ => DataType::Anything,
                    },
                };
                self.insert_var(variable.clone(), item_type, true);

                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::Expression(expr) => {
                self.check_expression(expr)?;
            }
            Statement::Match {
                value,
                cases,
                default,
            } => {
                let value_type = self.check_expression(value)?;
                for (case_expr, case_body) in cases.iter_mut() {
                    if !Self::is_match_identifier_pattern(case_expr) {
                        let case_type = self.check_match_pattern(case_expr)?;
                        if value_type != DataType::Unknown
                            && case_type != DataType::Unknown
                            && !self.is_assignable(&value_type, &case_type)
                        {
                            return Err(type_error(format!(
                                "Match case type mismatch: value is {:?}, case is {:?}",
                                value_type, case_type
                            )));
                        }
                    }

                    self.push_scope();

                    self.insert_match_pattern_bindings(case_expr);

                    self.check_statements(case_body)?;
                    self.pop_scope();
                }

                self.push_scope();
                self.check_statements(default)?;
                self.pop_scope();
            }
            Statement::Unsafe { body }
            | Statement::Module { body, .. }
            | Statement::DmireTable { body, .. }
            | Statement::DmireColumn { body, .. } => {
                self.push_scope();
                self.check_statements(body)?;
                self.pop_scope();
            }
            Statement::Asm { instructions } => {
                for (_, expr) in instructions.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            Statement::Drop { value } => {
                self.check_expression(value)?;
            }
            Statement::Move { target, value } => {
                let moved_type = self.check_expression(value)?;
                self.insert_var(target.clone(), moved_type, true);
            }
            Statement::Query {
                ops,
                bindings,
                group_by: _,
                joins: _,
                table: _,
            } => {
                for bind in bindings.iter() {
                    self.insert_var(bind.target.clone(), DataType::Anything, true);
                    self.insert_var(bind.alias.clone(), DataType::Anything, true);
                }

                for op in ops.iter_mut() {
                    self.check_query_op(op)?;
                }
            }
            Statement::DmireDlist { data, .. } => {
                for expr in data.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            Statement::Class { methods, .. } => self.check_statements(methods)?,
            Statement::Impl {
                trait_name,
                type_name,
                methods,
            } => {
                self.validate_impl_method_declarations(type_name, methods)?;
                if let Some(trait_name) = trait_name {
                    self.validate_trait_impl(trait_name, type_name, methods)?;
                }
                let old_self = self.impl_self_type.take();
                let old_self_name = self.impl_self_name.take();

                for method in methods.iter_mut() {
                    let has_self = matches!(
                        method,
                        Statement::Function { params, .. }
                            if params.iter().any(|(param_name, _)| param_name == "self")
                    );
                    self.impl_self_type = has_self.then(|| DataType::StructNamed(type_name.clone()));
                    self.impl_self_name = has_self.then(|| type_name.clone());
                    self.check_statement(method)?;
                }

                self.impl_self_type = old_self;
                self.impl_self_name = old_self_name;
            }
            Statement::Type { fields, .. } => self.check_statements(fields)?,
            Statement::Code { methods, .. } => self.check_statements(methods)?,
            Statement::Skill { name, methods } => {
                if methods.is_empty() {
                    return Err(type_error(format!(
                        "Skill '{}' must declare at least one method",
                        name
                    )));
                }
                self.validate_trait_method_declarations(name, methods, "Skill")?;
            }
            Statement::Trait { name, methods } => {
                self.validate_trait_method_declarations(name, methods, "Trait")?;
            }
            Statement::Break
            | Statement::Continue
            | Statement::ExternLib { .. }
            | Statement::ExternFunction { .. }
            | Statement::Enum { .. } => {}
            Statement::AddLib { .. } => {}
            Statement::Use { path, .. } => {
                if path == "__std_all__" {
                    for module in ["math", "term", "strings", "lists", "dicts", "time"] {
                        self.import_std_members(module);
                    }
                } else if let Some(rest) = path.strip_prefix("stdall:") {
                    self.import_std_members(rest);
                } else if let Some(rest) = path.strip_prefix("stdselect:") {
                    if let Some((_, items)) = rest.split_once(':') {
                        for item in items.split(',').filter(|item| !item.is_empty()) {
                            self.insert_var(item.to_string(), DataType::Anything, true);
                        }
                    }
                } else if let Some(rest) = path.strip_prefix("stdalias:") {
                    if let Some((alias, _)) = rest.split_once(':') {
                        self.insert_var(alias.to_string(), DataType::Anything, true);
                    }
                } else if let Some(rest) = path.strip_prefix("stdaliasselect:") {
                    let mut parts = rest.splitn(3, ':');
                    if let Some(alias) = parts.next() {
                        self.insert_var(alias.to_string(), DataType::Anything, true);
                    }
                }
            }
        }

        Ok(())
    }

    fn check_query_op(&mut self, op: &mut QueryOp) -> Result<()> {
        match op {
            QueryOp::Insert { assigns } => {
                for (_, expr) in assigns.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Update { condition, assigns } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error(format!(
                        "Query update condition must be bool, got {:?}",
                        cond_type
                    )));
                }
                for (_, expr) in assigns.iter_mut() {
                    self.check_expression(expr)?;
                }
            }
            QueryOp::Delete { condition } => {
                let cond_type = self.check_expression(condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error(format!(
                        "Query delete condition must be bool, got {:?}",
                        cond_type
                    )));
                }
            }
            QueryOp::Get(get) => {
                let cond_type = self.check_expression(&mut get.condition)?;
                if !Self::is_bool_like(&cond_type) {
                    return Err(type_error(format!(
                        "Query get condition must be bool, got {:?}",
                        cond_type
                    )));
                }

                self.push_scope();
                self.insert_var(get.target.clone(), DataType::Anything, true);
                self.check_statements(&mut get.body)?;
                self.pop_scope();
            }
            QueryOp::Export { .. } | QueryOp::Import { .. } => {}
        }

        Ok(())
    }

    fn check_expression(&mut self, expression: &mut Expression) -> Result<DataType> {
        match expression {
            Expression::Literal(lit) => {
                if let Literal::Int(value) = lit {
                    if let Some(scope) = self.scopes.last() {
                        for (_name, (dt, _)) in scope.iter() {
                            if let DataType::I8
                            | DataType::I16
                            | DataType::I32
                            | DataType::U8
                            | DataType::U16
                            | DataType::U32 = dt
                            {
                                Self::validate_int_literal_range(dt, *value)?;
                            }
                        }
                    }
                }
                Ok(Self::literal_type(lit))
            }
            Expression::Identifier(ident) => {
                let (resolved, _) = self.lookup_var(&ident.name).ok_or_else(|| {
                    type_error_at(
                        ident.line,
                        ident.column,
                        format!("Unknown identifier '{}'", ident.name),
                    )
                })?;
                ident.data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } => {
                let left_type = if Self::is_logical_operator(operator) {
                    self.check_expression_allow_unknown_identifier(left)?
                } else {
                    self.check_expression(left)?
                };
                let right_type = if Self::is_logical_operator(operator) {
                    self.check_expression_allow_unknown_identifier(right)?
                } else {
                    self.check_expression(right)?
                };
                let resolved = self.resolve_binary_type(operator, &left_type, &right_type)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::UnaryOp {
                operator,
                operand,
                data_type,
            } => {
                let operand_type = self.check_expression(operand)?;
                let resolved = match operator.as_str() {
                    "-" if Self::is_numeric(&operand_type) => operand_type,
                    "not" | "!" if Self::is_bool_like(&operand_type) => DataType::Bool,
                    "-" => {
                        return Err(type_error(format!(
                            "Unary '-' requires numeric operand, got {:?}",
                            operand_type
                        )));
                    }
                    _ => DataType::Unknown,
                };
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::NamedArg {
                value, data_type, ..
            } => {
                let resolved = self.check_expression(value)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::Call {
                name,
                args,
                data_type,
            } => {
                // Special handling for explicit typed input-style builtins.
                if matches!(name.as_str(), "ireru" | "input" | "std.input")
                    && *data_type != DataType::Unknown
                {
                    *data_type = data_type.clone();
                    return Ok(data_type.clone());
                }

                let arg_types: Vec<DataType> = args
                    .iter_mut()
                    .map(|arg| self.check_expression(arg))
                    .collect::<Result<_>>()?;

                if name == "__if_expr" {
                    if args.len() != 3 {
                        return Err(type_error(
                            "__if_expr expects condition, then branch, and else branch"
                                .to_string(),
                        ));
                    }

                    let cond_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
                    if !Self::is_bool_like(&cond_type) {
                        return Err(type_error(format!(
                            "If expression condition must be bool, got {:?}",
                            cond_type
                        )));
                    }

                    let then_type = Self::closure_return_type(&args[1], "__if_expr then")?;
                    let else_type = Self::closure_return_type(&args[2], "__if_expr else")?;
                    let resolved = Self::unify_types(&then_type, &else_type)?;
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if let Some(resolved) = self.resolve_instance_method_call(name, &arg_types)? {
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if name == "dicts.get" {
                    let resolved = match arg_types.first().cloned().unwrap_or(DataType::Unknown) {
                        DataType::Map { value_type, .. } => *value_type,
                        DataType::Dict => arg_types.get(2).cloned().unwrap_or(DataType::Anything),
                        _ => arg_types.get(2).cloned().unwrap_or(DataType::Anything),
                    };
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if name == "dicts.set" {
                    let key_type = arg_types.get(1).cloned().unwrap_or(DataType::Anything);
                    let value_type = arg_types.get(2).cloned().unwrap_or(DataType::Anything);
                    let resolved = match arg_types.first().cloned().unwrap_or(DataType::Unknown) {
                        DataType::Map {
                            key_type,
                            value_type: existing_value,
                        } => DataType::Map {
                            key_type,
                            value_type: Box::new(
                                Self::unify_types(&existing_value, &value_type)
                                    .unwrap_or(value_type.clone()),
                            ),
                        },
                        _ => DataType::Map {
                            key_type: Box::new(key_type),
                            value_type: Box::new(value_type),
                        },
                    };
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if name == "lists.get" {
                    let resolved = match arg_types.first().cloned().unwrap_or(DataType::Unknown) {
                        DataType::Vector { element_type, .. } => *element_type,
                        DataType::List => DataType::Anything,
                        other => {
                            return Err(type_error(format!(
                                "lists.get expects vec/vec! input, got {:?}",
                                other
                            )));
                        }
                    };
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if name == "lists.push" {
                    let list_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
                    let value_type = arg_types.get(1).cloned().unwrap_or(DataType::Unknown);
                    let resolved = match list_type {
                        DataType::Vector {
                            element_type,
                            dynamic: true,
                        } => DataType::Vector {
                            element_type: Box::new(
                                Self::unify_types(&element_type, &value_type)
                                    .unwrap_or(value_type.clone()),
                            ),
                            dynamic: true,
                        },
                        DataType::Vector { dynamic: false, .. } => {
                            return Err(type_error(
                                "lists.push requires a dynamic vector declared as vec![T]"
                                    .to_string(),
                            ));
                        }
                        DataType::Unknown => DataType::Vector {
                            element_type: Box::new(value_type),
                            dynamic: true,
                        },
                        other => {
                            return Err(type_error(format!(
                                "lists.push expects vec![T], got {:?}",
                                other
                            )));
                        }
                    };
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if name == "lists.slice" {
                    let list_type = arg_types.first().cloned().unwrap_or(DataType::Unknown);
                    let resolved = match list_type {
                        DataType::Vector { element_type, .. } => DataType::Vector {
                            element_type: element_type.clone(),
                            dynamic: true,
                        },
                        DataType::List => DataType::Vector {
                            element_type: Box::new(DataType::Unknown),
                            dynamic: true,
                        },
                        other => {
                            return Err(type_error(format!(
                                "lists.slice expects vector input, got {:?}",
                                other
                            )));
                        }
                    };
                    *data_type = resolved.clone();
                    return Ok(resolved);
                }

                if let Some(sig) = self.functions.get(name).cloned() {
                    if sig.params.len() != arg_types.len() {
                        return Err(type_error(format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            sig.params.len(),
                            arg_types.len()
                        )));
                    }

                    for (idx, (expected, actual)) in
                        sig.params.iter().zip(arg_types.iter()).enumerate()
                    {
                        if !self.is_assignable(expected, actual) {
                            return Err(type_error(format!(
                                "Function '{}' argument {} expects {:?}, got {:?}",
                                name,
                                idx + 1,
                                expected,
                                actual
                            )));
                        }
                    }

                    *data_type = sig.return_type.clone();
                    return Ok(sig.return_type);
                }

                if let Some(ret) = self.builtin_returns.get(name).cloned() {
                    *data_type = ret.clone();
                    return Ok(ret);
                }

                if let Some(rest) = name.strip_prefix("std.") {
                    if let Some(ret) = self.builtin_returns.get(rest).cloned() {
                        *data_type = ret.clone();
                        return Ok(ret);
                    }
                }

                if let Some(sig) = self.functions.get(name).cloned() {
                    if sig.params.len() != args.len() {
                        return Err(type_error(format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            sig.params.len(),
                            args.len()
                        )));
                    }
                    *data_type = sig.return_type.clone();
                    return Ok(sig.return_type);
                }

                // Check class constructors BEFORE enum variants.
                // Associated methods use `Type::method(...)`, while direct constructors
                // still use `(Type field: value, ...)`.
                if let Some(class_sig) = self.classes.get(name).cloned() {
                    self.check_class_constructor_call(name, &class_sig, args, &arg_types)?;
                    *data_type = DataType::StructNamed(name.clone());
                    return Ok(DataType::StructNamed(name.clone()));
                }

                if let Some(variant_sig) = self.enum_variants.get(name).cloned() {
                    self.check_enum_variant_call(name, &variant_sig, &arg_types)?;
                    let enum_name = name
                        .split_once('.')
                        .map(|(enum_name, _)| enum_name.to_string())
                        .unwrap_or_else(|| name.clone());
                    *data_type = DataType::EnumNamed(enum_name.clone());
                    return Ok(DataType::EnumNamed(enum_name));
                }

                Err(type_error(format!("Unknown function '{}'", name)))
            }
            Expression::List {
                elements,
                element_type,
                data_type,
            } => {
                let mut current = DataType::Unknown;
                for element in elements.iter_mut() {
                    let elem_type = self.check_expression(element)?;
                    current = Self::unify_types(&current, &elem_type)?;
                }
                *element_type = current.clone();
                *data_type = DataType::Vector {
                    element_type: Box::new(current.clone()),
                    dynamic: false,
                };
                Ok(data_type.clone())
            }
            Expression::Dict { entries, data_type } => {
                let mut key_type = DataType::Unknown;
                let mut value_type = DataType::Unknown;
                for (key, value) in entries.iter_mut() {
                    let next_key = self.check_expression(key)?;
                    let next_value = self.check_expression(value)?;
                    key_type = Self::unify_types(&key_type, &next_key)?;
                    value_type = Self::unify_types(&value_type, &next_value)?;
                }
                *data_type = DataType::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                };
                Ok(data_type.clone())
            }
            Expression::Tuple {
                elements,
                data_type,
            } => {
                for element in elements.iter_mut() {
                    self.check_expression(element)?;
                }
                *data_type = DataType::Tuple;
                Ok(DataType::Tuple)
            }
            Expression::Index {
                target,
                index,
                data_type,
            } => {
                let target_type = self.check_expression(target)?;
                let index_type = self.check_expression(index)?;

                if !Self::is_numeric(&index_type)
                    && !matches!(target_type, DataType::Dict)
                    && index_type != DataType::Unknown
                {
                    return Err(type_error(format!(
                        "Index must be numeric for {:?}, got {:?}",
                        target_type, index_type
                    )));
                }

                let resolved = match target_type {
                    DataType::Array { element_type, .. } | DataType::Slice { element_type } => {
                        *element_type
                    }
                    DataType::Str => DataType::Str,
                    DataType::Vector { element_type, .. } => *element_type,
                    DataType::List | DataType::Tuple | DataType::Dict => DataType::Anything,
                    DataType::Map { value_type, .. } => *value_type,
                    DataType::Unknown => DataType::Unknown,
                    other => {
                        return Err(type_error(format!("Type {:?} is not indexable", other)));
                    }
                };

                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::MemberAccess {
                target,
                member,
                data_type,
            } => {
                let target_type = self.check_expression(target)?;
                if target_type.is_struct_like() {
                    if let Some(struct_name) = self.struct_name_for_expr(target).or_else(|| {
                        target_type.struct_name().map(ToOwned::to_owned)
                    }) {
                        if let Some(class_sig) = self.classes.get(&struct_name) {
                            if let Some(field) = class_sig.fields.iter().find(|f| f.name == *member)
                            {
                                *data_type = field.data_type.clone();
                                return Ok(field.data_type.clone());
                            }
                        }
                        if let Some(fn_sig) =
                            self.functions.get(&format!("{}.{}", struct_name, member))
                        {
                            *data_type = fn_sig.return_type.clone();
                            return Ok(fn_sig.return_type.clone());
                        }
                        return Err(type_error(format!(
                            "Struct '{}' has no field or method '{}'",
                            struct_name, member
                        )));
                    }

                    for class_sig in self.classes.values() {
                        if let Some(field) = class_sig.fields.iter().find(|f| f.name == *member) {
                            *data_type = field.data_type.clone();
                            return Ok(field.data_type.clone());
                        }
                    }
                    for (fn_name, fn_sig) in &self.functions {
                        if let Some((_base_type, method_name)) = fn_name.split_once('.') {
                            if method_name == *member {
                                *data_type = fn_sig.return_type.clone();
                                return Ok(fn_sig.return_type.clone());
                            }
                        }
                    }
                }
                *data_type = DataType::Anything;
                Ok(DataType::Anything)
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
            } => {
                let full_name = format!("{}.{}", enum_name, variant_name);
                if !self.enum_variants.contains_key(&full_name) {
                    return Err(type_error(format!("Unknown enum variant '{}'", full_name)));
                }
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
            } => {
                let mut arg_types = Vec::with_capacity(payloads.len());
                for payload in payloads.iter_mut() {
                    arg_types.push(self.check_expression(payload)?);
                }
                let full_name = format!("{}.{}", enum_name, variant_name);
                let variant_sig =
                    self.enum_variants.get(&full_name).cloned().ok_or_else(|| {
                        type_error(format!("Unknown enum variant '{}'", full_name))
                    })?;
                self.check_enum_variant_call(&full_name, &variant_sig, &arg_types)?;
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => {
                self.push_scope();

                for (name, value) in capture.iter() {
                    self.insert_var(name.clone(), Self::mire_value_type(value), true);
                }

                for (name, ptype) in params.iter() {
                    self.insert_var(name.clone(), ptype.clone(), true);
                }

                self.return_type_stack.push(return_type.clone());
                self.check_statements(body)?;
                let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);

                if *return_type == DataType::Unknown {
                    *return_type = inferred_return;
                }

                self.pop_scope();
                Ok(DataType::Function)
            }
            Expression::Reference {
                expr,
                is_mutable,
                data_type,
            } => {
                self.check_expression(expr)?;
                *data_type = if *is_mutable {
                    DataType::RefMut
                } else {
                    DataType::Ref
                };
                Ok(data_type.clone())
            }
            Expression::Dereference { expr, data_type } => {
                let inner = self.check_expression(expr)?;
                let resolved = match inner {
                    DataType::Ref | DataType::RefMut => DataType::Anything,
                    DataType::Unknown => DataType::Unknown,
                    other => {
                        return Err(type_error(format!(
                            "Cannot dereference non-reference type {:?}",
                            other
                        )));
                    }
                };
                *data_type = resolved.clone();
                Ok(resolved)
            }
            Expression::Box { value, data_type } => {
                self.check_expression(value)?;
                *data_type = DataType::Box;
                Ok(DataType::Box)
            }
            Expression::Pipeline {
                input,
                stage,
                safe,
                data_type,
            } => {
                let _ = self.check_expression(input)?;
                let _ = self.check_expression(stage)?;
                let _ = safe;
                Ok(data_type.clone())
            }
            Expression::Match {
                value,
                cases,
                default,
                data_type,
            } => {
                let _ = self.check_expression(value)?;
                for (case_expr, case_body) in cases.iter_mut() {
                    if !Self::is_match_identifier_pattern(case_expr) {
                        let _ = self.check_match_pattern(case_expr)?;
                    }

                    self.push_scope();

                    self.insert_match_pattern_bindings(case_expr);

                    let _ = self.check_expression(case_body)?;
                    self.pop_scope();
                }
                let _ = self.check_expression(default)?;
                Ok(data_type.clone())
            }
        }
    }

    fn resolve_binary_type(
        &self,
        operator: &str,
        left: &DataType,
        right: &DataType,
    ) -> Result<DataType> {
        match operator {
            "+" | "-" | "*" | "/" | "%" => {
                if operator == "+" && left == &DataType::Str && right == &DataType::Str {
                    return Ok(DataType::Str);
                }

                if operator == "+" {
                    match (left, right) {
                        (
                            DataType::Vector {
                                element_type: l_elem,
                                dynamic: l_dyn,
                            },
                            DataType::Vector {
                                element_type: r_elem,
                                dynamic: r_dyn,
                            },
                        ) => {
                            let unified_elem = Self::unify_types(l_elem, r_elem)?;
                            return Ok(DataType::Vector {
                                element_type: Box::new(unified_elem),
                                dynamic: *l_dyn || *r_dyn,
                            });
                        }
                        (DataType::Vector { .. }, DataType::List)
                        | (DataType::List, DataType::Vector { .. })
                        | (DataType::List, DataType::List) => {
                            return Ok(DataType::Vector {
                                element_type: Box::new(DataType::Unknown),
                                dynamic: true,
                            });
                        }
                        _ => {}
                    }
                }

                if Self::is_numeric(left) && Self::is_numeric(right) {
                    return Ok(Self::promote_numeric(left, right));
                }

                Err(type_error(format!(
                    "Operator '{}' not supported for {:?} and {:?}",
                    operator, left, right
                )))
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => Ok(DataType::Bool),
            "and" | "or" | "xor" | "&&" | "||" => {
                if left == &DataType::Unknown || right == &DataType::Unknown {
                    return Ok(DataType::Bool);
                }
                if Self::is_bool_like(left) && Self::is_bool_like(right) {
                    Ok(DataType::Bool)
                } else {
                    Err(type_error(format!(
                        "Logical operator '{}' requires bool operands, got {:?} and {:?}",
                        operator, left, right
                    )))
                }
            }
            _ => Ok(DataType::Unknown),
        }
    }

    fn check_expression_allow_unknown_identifier(
        &mut self,
        expression: &mut Expression,
    ) -> Result<DataType> {
        match expression {
            Expression::Identifier(ident) => {
                if let Some((resolved, _)) = self.lookup_var(&ident.name) {
                    ident.data_type = resolved.clone();
                    Ok(resolved)
                } else {
                    ident.data_type = DataType::Unknown;
                    Ok(DataType::Unknown)
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } if Self::is_logical_operator(operator) => {
                let left_type = self.check_expression_allow_unknown_identifier(left)?;
                let right_type = self.check_expression_allow_unknown_identifier(right)?;
                let resolved = self.resolve_binary_type(operator, &left_type, &right_type)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            _ => self.check_expression(expression),
        }
    }

    fn is_logical_operator(operator: &str) -> bool {
        matches!(operator, "and" | "or" | "xor" | "&&" | "||")
    }

    fn is_match_identifier_pattern(expression: &Expression) -> bool {
        matches!(expression, Expression::Identifier(_))
    }

    fn literal_type(lit: &Literal) -> DataType {
        match lit {
            Literal::Int(_) => DataType::I64,
            Literal::Float(_) => DataType::F64,
            Literal::Str(_) => DataType::Str,
            Literal::Bool(_) => DataType::Bool,
            Literal::None => DataType::None,
            Literal::List(_) => DataType::Vector {
                element_type: Box::new(DataType::Unknown),
                dynamic: false,
            },
            Literal::Dict(_) => DataType::Map {
                key_type: Box::new(DataType::Unknown),
                value_type: Box::new(DataType::Unknown),
            },
            Literal::Tuple(_) => DataType::Tuple,
        }
    }

    fn validate_int_literal_range(data_type: &DataType, value: i64) -> Result<()> {
        match data_type {
            DataType::I8 => {
                if value < -128 || value > 127 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds i8 range (-128 to 127)",
                        value
                    )));
                }
            }
            DataType::I16 => {
                if value < -32768 || value > 32767 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds i16 range (-32768 to 32767)",
                        value
                    )));
                }
            }
            DataType::I32 => {
                if value < -2147483648 || value > 2147483647 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds i32 range (-2147483648 to 2147483647)",
                        value
                    )));
                }
            }
            DataType::U8 => {
                if value < 0 || value > 255 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds u8 range (0 to 255)",
                        value
                    )));
                }
            }
            DataType::U16 => {
                if value < 0 || value > 65535 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds u16 range (0 to 65535)",
                        value
                    )));
                }
            }
            DataType::U32 => {
                if value < 0 || value > 4294967295 {
                    return Err(type_error(format!(
                        "Integer literal {} exceeds u32 range (0 to 4294967295)",
                        value
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn mire_value_type(value: &MireValue) -> DataType {
        match value {
            MireValue::I8(_) => DataType::I8,
            MireValue::I16(_) => DataType::I16,
            MireValue::I32(_) => DataType::I32,
            MireValue::I64(_) => DataType::I64,
            MireValue::U8(_) => DataType::U8,
            MireValue::U16(_) => DataType::U16,
            MireValue::U32(_) => DataType::U32,
            MireValue::U64(_) => DataType::U64,
            MireValue::Float(_) => DataType::F64,
            MireValue::F64(_) => DataType::F64,
            MireValue::F32(_) => DataType::F32,
            MireValue::Str(_) => DataType::Str,
            MireValue::Bool(_) => DataType::Bool,
            MireValue::None => DataType::None,
            MireValue::List(values) => {
                let element_type = values
                    .first()
                    .map(Self::mire_value_type)
                    .unwrap_or(DataType::Anything);
                DataType::Vector {
                    element_type: Box::new(element_type),
                    dynamic: false,
                }
            }
            MireValue::Dict(entries) => {
                let key_type = entries
                    .first()
                    .map(|((key, _), _)| Self::mire_value_type(key))
                    .unwrap_or(DataType::Anything);
                let value_type = entries
                    .first()
                    .map(|((_, value), _)| Self::mire_value_type(value))
                    .unwrap_or(DataType::Anything);
                DataType::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                }
            }
            MireValue::Tuple(_) => DataType::Tuple,
            MireValue::Function(_) | MireValue::Builtinfn(_) => DataType::Function,
            MireValue::Object { .. } | MireValue::Instance { .. } => DataType::Anything,
            MireValue::Trait { .. } => DataType::DynTrait {
                trait_name: "trait".to_string(),
            },
            MireValue::Ref { is_mutable, .. } => {
                if *is_mutable {
                    DataType::RefMut
                } else {
                    DataType::Ref
                }
            }
            MireValue::Box { .. } => DataType::Box,
            MireValue::Array { elements, size } => {
                let element_type = elements
                    .first()
                    .map(Self::mire_value_type)
                    .unwrap_or(DataType::Anything);
                DataType::Array {
                    element_type: Box::new(element_type),
                    size: *size,
                }
            }
            MireValue::Slice { elements } => {
                let element_type = elements
                    .first()
                    .map(Self::mire_value_type)
                    .unwrap_or(DataType::Anything);
                DataType::Slice {
                    element_type: Box::new(element_type),
                }
            }
            MireValue::EnumVariant { enum_name, .. } => DataType::EnumNamed(enum_name.clone()),
        }
    }

    fn unify_types(left: &DataType, right: &DataType) -> Result<DataType> {
        if left == right {
            return Ok(left.clone());
        }

        if left.is_struct_like() && right.is_struct_like() {
            return match (left.struct_name(), right.struct_name()) {
                (Some(left_name), Some(right_name)) if left_name != right_name => Err(type_error(
                    format!(
                        "Cannot unify incompatible struct types {:?} and {:?}",
                        left, right
                    ),
                )),
                (Some(_), _) => Ok(left.clone()),
                (_, Some(_)) => Ok(right.clone()),
                _ => Ok(DataType::Struct),
            };
        }

        if left.is_enum_like() && right.is_enum_like() {
            return match (left.enum_name(), right.enum_name()) {
                (Some(left_name), Some(right_name)) if left_name != right_name => Err(type_error(
                    format!("Cannot unify incompatible enum types {:?} and {:?}", left, right),
                )),
                (Some(_), _) => Ok(left.clone()),
                (_, Some(_)) => Ok(right.clone()),
                _ => Ok(DataType::Enum),
            };
        }

        if left == &DataType::Unknown {
            return Ok(right.clone());
        }
        if right == &DataType::Unknown {
            return Ok(left.clone());
        }

        if Self::is_numeric(left) && Self::is_numeric(right) {
            return Ok(Self::promote_numeric(left, right));
        }

        match (left, right) {
            (
                DataType::Vector {
                    element_type: left_elem,
                    dynamic: left_dynamic,
                },
                DataType::Vector {
                    element_type: right_elem,
                    dynamic: right_dynamic,
                },
            ) => {
                let element_type = Self::unify_types(left_elem, right_elem)?;
                return Ok(DataType::Vector {
                    element_type: Box::new(element_type),
                    dynamic: *left_dynamic || *right_dynamic,
                });
            }
            (
                DataType::Map {
                    key_type: left_key,
                    value_type: left_value,
                },
                DataType::Map {
                    key_type: right_key,
                    value_type: right_value,
                },
            ) => {
                let key_type = Self::unify_types(left_key, right_key)?;
                let value_type = Self::unify_types(left_value, right_value)?;
                return Ok(DataType::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                });
            }
            _ => {}
        }

        Err(type_error(format!(
            "Cannot unify incompatible types {:?} and {:?}",
            left, right
        )))
    }

    fn promote_numeric(left: &DataType, right: &DataType) -> DataType {
        if matches!(left, DataType::F64 | DataType::F32)
            || matches!(right, DataType::F64 | DataType::F32)
        {
            DataType::F64
        } else if left == &DataType::I64 || right == &DataType::I64 {
            DataType::I64
        } else {
            left.clone()
        }
    }

    fn is_numeric(dtype: &DataType) -> bool {
        matches!(
            dtype,
            DataType::I8
                | DataType::I16
                | DataType::I32
                | DataType::I64
                | DataType::U8
                | DataType::U16
                | DataType::U32
                | DataType::U64
                | DataType::F32
                | DataType::F64
        )
    }

    fn is_bool_like(dtype: &DataType) -> bool {
        matches!(
            dtype,
            DataType::Bool | DataType::Anything | DataType::Unknown
        )
    }

    fn is_assignable(&self, expected: &DataType, actual: &DataType) -> bool {
        if expected == actual {
            return true;
        }

        if expected.is_struct_like() && actual.is_struct_like() {
            return match (expected.struct_name(), actual.struct_name()) {
                (Some(expected_name), Some(actual_name)) => expected_name == actual_name,
                _ => true,
            };
        }

        if expected.is_enum_like() && actual.is_enum_like() {
            return match (expected.enum_name(), actual.enum_name()) {
                (Some(expected_name), Some(actual_name)) => expected_name == actual_name,
                _ => true,
            };
        }

        if expected == &DataType::Anything || actual == &DataType::Unknown {
            return true;
        }

        if expected == &DataType::Unknown {
            return true;
        }

        if expected == &DataType::Dict && actual == &DataType::List {
            return true;
        }

        if let DataType::Map { .. } = expected {
            match actual {
                DataType::List | DataType::Vector { .. } => return true,
                _ => {}
            }
        }

        match expected {
            DataType::Array { .. } | DataType::Slice { .. } => {
                return matches!(actual, DataType::Vector { .. } | DataType::List);
            }
            _ => {}
        }

        if let DataType::Vector { .. } = expected {
            match actual {
                DataType::List | DataType::Vector { .. } => return true,
                _ => {}
            }
        }

        Self::is_numeric(expected) && Self::is_numeric(actual)
    }

    fn validate_explicit_nested_literal(expected: &DataType, expr: &Expression) -> Result<()> {
        match (expected, expr) {
            (
                DataType::Vector { element_type, .. } | DataType::Array { element_type, .. },
                Expression::List { elements, .. },
            ) => {
                if Self::requires_explicit_nested_element(element_type) {
                    for element in elements {
                        if !matches!(
                            element,
                            Expression::List { .. }
                                | Expression::Dict { .. }
                                | Expression::Identifier(_)
                        ) {
                            return Err(type_error(format!(
                                "Nested literal for {:?} must use explicit inner brackets",
                                expected
                            )));
                        }
                    }
                }
                for element in elements {
                    Self::validate_explicit_nested_literal(element_type, element)?;
                }
                Ok(())
            }
            (DataType::Map { value_type, .. }, Expression::Dict { entries, .. }) => {
                if Self::requires_explicit_nested_element(value_type) {
                    for (_, value) in entries {
                        if !matches!(value, Expression::List { .. } | Expression::Dict { .. }) {
                            return Err(type_error(format!(
                                "Nested literal for {:?} must use explicit inner brackets",
                                expected
                            )));
                        }
                    }
                }
                for (_, value) in entries {
                    Self::validate_explicit_nested_literal(value_type, value)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn closure_return_type(expr: &Expression, context: &str) -> Result<DataType> {
        if let Expression::Closure { return_type, .. } = expr {
            Ok(return_type.clone())
        } else {
            Err(type_error(format!(
                "{} must be represented as a closure in the AST",
                context
            )))
        }
    }

    fn requires_explicit_nested_element(dtype: &DataType) -> bool {
        matches!(
            dtype,
            DataType::Vector { .. } | DataType::Array { .. } | DataType::Map { .. }
        )
    }

    fn check_enum_variant_call(
        &self,
        variant_name: &str,
        variant_sig: &EnumVariantSig,
        arg_types: &[DataType],
    ) -> Result<()> {
        if variant_sig.payload_types.len() != arg_types.len() {
            return Err(type_error(format!(
                "Enum variant '{}' expects {} values, got {}",
                variant_name,
                variant_sig.payload_types.len(),
                arg_types.len()
            )));
        }

        for (index, (expected, actual)) in variant_sig
            .payload_types
            .iter()
            .zip(arg_types.iter())
            .enumerate()
        {
            if !self.is_assignable(expected, actual) {
                return Err(type_error(format!(
                    "Enum variant '{}' value {} expects {:?}, got {:?}",
                    variant_name,
                    index + 1,
                    expected,
                    actual
                )));
            }
        }

        Ok(())
    }

    fn check_match_pattern(&mut self, pattern: &mut Expression) -> Result<DataType> {
        match pattern {
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
            } => {
                let full_name = format!("{}.{}", enum_name, variant_name);
                if !self.enum_variants.contains_key(&full_name) {
                    return Err(type_error(format!("Unknown enum variant '{}'", full_name)));
                }
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
            } => {
                let full_name = format!("{}.{}", enum_name, variant_name);
                let variant_sig =
                    self.enum_variants.get(&full_name).cloned().ok_or_else(|| {
                        type_error(format!("Unknown enum variant '{}'", full_name))
                    })?;
                let mut arg_types = Vec::with_capacity(payloads.len());
                for (index, payload) in payloads.iter_mut().enumerate() {
                    if matches!(payload, Expression::Identifier(_)) {
                        arg_types.push(
                            variant_sig
                                .payload_types
                                .get(index)
                                .cloned()
                                .unwrap_or(DataType::Unknown),
                        );
                    } else {
                        arg_types.push(self.check_expression(payload)?);
                    }
                }
                self.check_enum_variant_call(&full_name, &variant_sig, &arg_types)?;
                *data_type = DataType::EnumNamed(enum_name.clone());
                Ok(DataType::EnumNamed(enum_name.clone()))
            }
            _ => self.check_expression(pattern),
        }
    }

    fn insert_match_pattern_bindings(&mut self, case_expr: &Expression) {
        if let Expression::EnumVariant {
            enum_name,
            variant_name,
            payloads,
            ..
        } = case_expr
        {
            let full_name = format!("{}.{}", enum_name, variant_name);
            if let Some(variant_sig) = self.enum_variants.get(&full_name).cloned() {
                for (payload_expr, payload_type) in
                    payloads.iter().zip(variant_sig.payload_types.iter())
                {
                    if let Expression::Identifier(id) = payload_expr {
                        self.insert_var(id.name.clone(), payload_type.clone(), true);
                    }
                }
            }
        }
    }

    fn validate_trait_impl(
        &self,
        trait_name: &str,
        type_name: &str,
        methods: &[Statement],
    ) -> Result<()> {
        let trait_sig = self
            .traits
            .get(trait_name)
            .ok_or_else(|| type_error(format!("Unknown skill/trait '{}'", trait_name)))?;

        for required_method in &trait_sig.methods {
            let implemented = methods.iter().find_map(|statement| match statement {
                Statement::Function {
                    name,
                    params,
                    return_type,
                    ..
                } if name == &required_method.name => Some((params.clone(), return_type.clone())),
                _ => None,
            });

            let Some((implemented_params, implemented_return)) = implemented else {
                return Err(type_error(format!(
                    "Type '{}' does not implement required method '{}.{}'",
                    type_name, trait_name, required_method.name
                )));
            };

            let required_kind = Self::method_kind_for_params(&required_method.params);
            let implemented_kind = Self::method_kind_for_params(&implemented_params);
            if required_kind != implemented_kind {
                return Err(type_error(format!(
                    "Method '{}.{}' must be implemented as {}, got {}",
                    trait_name,
                    required_method.name,
                    Self::describe_method_kind(required_kind),
                    Self::describe_method_kind(implemented_kind),
                )));
            }

            let required_params =
                Self::normalize_trait_impl_params(type_name, &required_method.params);
            let implemented_params =
                Self::normalize_trait_impl_params(type_name, &implemented_params);

            if implemented_params != required_params
                || implemented_return != required_method.return_type
            {
                return Err(type_error(format!(
                    "Method '{}.{}' implementation signature does not match declaration: expected {:?} -> {:?}, got {:?} -> {:?}",
                    trait_name,
                    required_method.name,
                    required_params,
                    required_method.return_type,
                    implemented_params,
                    implemented_return,
                )));
            }
        }

        Ok(())
    }

    fn validate_trait_method_declarations(
        &self,
        container_name: &str,
        methods: &[TraitMethodSig],
        container_kind: &str,
    ) -> Result<()> {
        for method in methods {
            Self::validate_self_param_position(
                &method.params,
                format!(
                    "{} '{}.{}'",
                    container_kind, container_name, method.name
                ),
            )?;
        }
        Ok(())
    }

    fn validate_impl_method_declarations(&self, type_name: &str, methods: &[Statement]) -> Result<()> {
        for method in methods {
            if let Statement::Function { name, params, .. } = method {
                Self::validate_self_param_position(
                    params,
                    format!("Method '{}.{}'", type_name, name),
                )?;
            }
        }
        Ok(())
    }

    fn normalize_trait_impl_params(
        owner_type_name: &str,
        params: &[(String, DataType)],
    ) -> Vec<DataType> {
        params
            .iter()
            .map(|(name, data_type)| {
                if name == "self" && matches!(data_type, DataType::Unknown | DataType::Struct) {
                    DataType::StructNamed(owner_type_name.to_string())
                } else {
                    data_type.clone()
                }
            })
            .collect()
    }

    fn validate_self_param_position(
        params: &[(String, DataType)],
        context: String,
    ) -> Result<()> {
        if params.iter().skip(1).any(|(name, _)| name == "self") {
            return Err(type_error(format!(
                "{} must declare 'self' as the first parameter",
                context
            )));
        }
        Ok(())
    }

    fn method_kind_for_params(params: &[(String, DataType)]) -> MethodKind {
        if params.first().is_some_and(|(name, _)| name == "self") {
            MethodKind::Instance
        } else {
            MethodKind::Associated
        }
    }

    fn describe_method_kind(kind: MethodKind) -> &'static str {
        match kind {
            MethodKind::Instance => "an instance method",
            MethodKind::Associated => "an associated method",
        }
    }

    fn check_class_constructor_call(
        &self,
        class_name: &str,
        class_sig: &ClassSig,
        args: &[Expression],
        arg_types: &[DataType],
    ) -> Result<()> {
        let has_named = args
            .iter()
            .any(|arg| matches!(arg, Expression::NamedArg { .. }));
        let has_positional = args
            .iter()
            .any(|arg| !matches!(arg, Expression::NamedArg { .. }));

        if has_named && has_positional {
            return Err(type_error(format!(
                "Constructor '{}' cannot mix named and positional arguments",
                class_name
            )));
        }

        if has_named {
            let mut seen = HashSet::new();
            for (index, arg) in args.iter().enumerate() {
                let Expression::NamedArg { name, .. } = arg else {
                    continue;
                };

                if !seen.insert(name.clone()) {
                    return Err(type_error(format!(
                        "Constructor '{}' received duplicate field '{}'",
                        class_name, name
                    )));
                }

                let field = class_sig
                    .fields
                    .iter()
                    .find(|field| field.name == *name)
                    .ok_or_else(|| {
                        type_error(format!(
                            "Constructor '{}' has no field '{}'",
                            class_name, name
                        ))
                    })?;

                let actual = arg_types.get(index).cloned().unwrap_or(DataType::Unknown);
                if !self.is_assignable(&field.data_type, &actual) {
                    return Err(type_error(format!(
                        "Constructor '{}.{}' expects {:?}, got {:?}",
                        class_name, name, field.data_type, actual
                    )));
                }
            }

            for field in &class_sig.fields {
                if !field.has_default && !seen.contains(&field.name) {
                    return Err(type_error(format!(
                        "Constructor '{}' is missing required field '{}'",
                        class_name, field.name
                    )));
                }
            }
        } else {
            if arg_types.len() > class_sig.fields.len() {
                return Err(type_error(format!(
                    "Constructor '{}' expects at most {} values, got {}",
                    class_name,
                    class_sig.fields.len(),
                    arg_types.len()
                )));
            }

            for (index, actual) in arg_types.iter().enumerate() {
                let Some(field) = class_sig.fields.get(index) else {
                    break;
                };
                if !self.is_assignable(&field.data_type, actual) {
                    return Err(type_error(format!(
                        "Constructor '{}.{}' expects {:?}, got {:?}",
                        class_name, field.name, field.data_type, actual
                    )));
                }
            }

            for field in class_sig.fields.iter().skip(arg_types.len()) {
                if !field.has_default {
                    return Err(type_error(format!(
                        "Constructor '{}' is missing required field '{}'",
                        class_name, field.name
                    )));
                }
            }
        }

        Ok(())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.struct_scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
        if self.struct_scopes.len() > 1 {
            self.struct_scopes.pop();
        }
    }

    fn insert_var(&mut self, name: String, data_type: DataType, is_mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (data_type, is_mutable));
        }
    }

    fn bind_struct_name(&mut self, name: &str, value: Option<&Expression>) {
        let struct_name = value.and_then(|expr| self.struct_name_for_expr(expr));
        if let Some(scope) = self.struct_scopes.last_mut() {
            if let Some(struct_name) = struct_name {
                scope.insert(name.to_string(), struct_name);
            } else {
                scope.remove(name);
            }
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(DataType, bool)> {
        if name == "self" {
            if let Some(ref self_type) = self.impl_self_type {
                return Some((self_type.clone(), true));
            }
        }
        for scope in self.scopes.iter().rev() {
            if let Some(data_type) = scope.get(name) {
                return Some(data_type.clone());
            }
        }
        None
    }

    fn lookup_struct_name(&self, name: &str) -> Option<String> {
        if name == "self" {
            return self.impl_self_name.clone();
        }
        for scope in self.struct_scopes.iter().rev() {
            if let Some(struct_name) = scope.get(name) {
                return Some(struct_name.clone());
            }
        }
        None
    }

    fn struct_name_for_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Call {
                name, data_type, ..
            } if data_type.is_struct_like() => {
                data_type
                    .struct_name()
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        if self.classes.contains_key(name) {
                            Some(name.clone())
                        } else if let Some((owner, _method)) = name.split_once('.') {
                            self.lookup_struct_name(owner).or_else(|| {
                                self.classes.contains_key(owner).then(|| owner.to_string())
                            })
                        } else {
                            None
                        }
                    })
            }
            Expression::Identifier(Identifier { name, .. }) => self.lookup_struct_name(name),
            _ => None,
        }
    }

    fn resolve_instance_method_call(
        &self,
        name: &str,
        arg_types: &[DataType],
    ) -> Result<Option<DataType>> {
        let Some((receiver_name, method_name)) = name.split_once('.') else {
            return Ok(None);
        };
        let Some(struct_name) = self.lookup_struct_name(receiver_name) else {
            return Ok(None);
        };
        let full_name = format!("{}.{}", struct_name, method_name);
        let Some(sig) = self.functions.get(&full_name) else {
            return Err(type_error(format!(
                "Struct '{}' has no method '{}'",
                struct_name, method_name
            )));
        };

        if !sig.params.first().is_some_and(DataType::is_struct_like) {
            return Ok(None);
        }

        let expected_args = sig.params.get(1..).unwrap_or(&[]);

        if expected_args.len() != arg_types.len() {
            return Err(type_error(format!(
                "Method '{}.{}' expects {} arguments, got {}",
                struct_name,
                method_name,
                expected_args.len(),
                arg_types.len()
            )));
        }

        for (idx, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
            if !self.is_assignable(expected, actual) {
                return Err(type_error(format!(
                    "Method '{}.{}' argument {} expects {:?}, got {:?}",
                    struct_name,
                    method_name,
                    idx + 1,
                    expected,
                    actual
                )));
            }
        }

        Ok(Some(sig.return_type.clone()))
    }
}

fn type_error(message: String) -> MireError {
    type_error_at(0, 0, message)
}

fn type_error_at(line: usize, column: usize, message: String) -> MireError {
    let source = TYPE_CHECKER_SOURCE.with(|source_guard| source_guard.borrow().clone());
    let (err_line, err_col) = if line == 0 { (1, 1) } else { (line, column) };
    let mut err = MireError::type_error_at(err_line, err_col, message);
    if let Some(src) = source {
        err = err.with_source(src);
    }
    err
}

#[cfg(test)]
mod tests {
    use super::check_program_types;
    use crate::parser::ast::{
        DataType, Expression, Identifier, Literal, Program, Statement, Visibility,
    };

    #[test]
    fn infers_unknown_let_from_literal() {
        let mut program = Program {
            statements: vec![Statement::Let {
                name: "x".to_string(),
                data_type: DataType::Unknown,
                value: Some(Expression::Literal(Literal::Int(42))),
                is_constant: false,
                is_mutable: false,
                is_static: false,
                visibility: Visibility::Public,
            }],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[0] {
            Statement::Let { data_type, .. } => assert_eq!(*data_type, DataType::I64),
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn resolves_identifier_type() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                },
                Statement::Expression(Expression::Identifier(Identifier {
                    name: "x".to_string(),
                    data_type: DataType::Unknown,
                    line: 0,
                    column: 0,
                })),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[1] {
            Statement::Expression(Expression::Identifier(ident)) => {
                assert_eq!(ident.data_type, DataType::I64)
            }
            _ => panic!("expected expression identifier"),
        }
    }

    #[test]
    fn infers_function_call_return_type() {
        let mut program = Program {
            statements: vec![
                Statement::Function {
                    name: "sum".to_string(),
                    params: vec![
                        ("a".to_string(), DataType::I64),
                        ("b".to_string(), DataType::I64),
                    ],
                    body: vec![Statement::Return(Some(Expression::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(Expression::Identifier(Identifier {
                            name: "a".to_string(),
                            data_type: DataType::Unknown,
                            line: 0,
                            column: 0,
                        })),
                        right: Box::new(Expression::Identifier(Identifier {
                            name: "b".to_string(),
                            data_type: DataType::Unknown,
                            line: 0,
                            column: 0,
                        })),
                        data_type: DataType::Unknown,
                    }))],
                    return_type: DataType::Unknown,
                    visibility: Visibility::Public,
                    is_method: false,
                },
                Statement::Expression(Expression::Call {
                    name: "sum".to_string(),
                    args: vec![
                        Expression::Literal(Literal::Int(1)),
                        Expression::Literal(Literal::Int(2)),
                    ],
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[1] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::I64)
            }
            _ => panic!("expected call expression"),
        }
    }

    #[test]
    fn fails_on_undefined_identifier() {
        let mut program = Program {
            statements: vec![Statement::Expression(Expression::Identifier(Identifier {
                name: "missing".to_string(),
                data_type: DataType::Unknown,
                line: 0,
                column: 0,
            }))],
        };

        let err = check_program_types(&mut program, "").expect_err("must fail");
        assert!(err.to_string().contains("Unknown identifier 'missing'"));
    }

    #[test]
    fn fails_on_assignment_type_mismatch() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                },
                Statement::Assignment {
                    target: "x".to_string(),
                    value: Expression::Literal(Literal::Str("bad".to_string())),
                    is_mutable: true,
                },
            ],
        };

        let err = check_program_types(&mut program, "").expect_err("must fail");
        assert!(
            err.to_string()
                .contains("Type mismatch in assignment to 'x'")
        );
    }

    #[test]
    fn accepts_builtin_calls() {
        let mut program = Program {
            statements: vec![
                Statement::Expression(Expression::Call {
                    name: "print".to_string(),
                    args: vec![Expression::Literal(Literal::Str("hello".to_string()))],
                    data_type: DataType::Unknown,
                }),
                Statement::Expression(Expression::Call {
                    name: "len".to_string(),
                    args: vec![Expression::Literal(Literal::List(vec![
                        Expression::Literal(Literal::Int(1)),
                        Expression::Literal(Literal::Int(2)),
                    ]))],
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");

        match &program.statements[0] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::None)
            }
            _ => panic!("expected call expression"),
        }
        match &program.statements[1] {
            Statement::Expression(Expression::Call { data_type, .. }) => {
                assert_eq!(*data_type, DataType::I64)
            }
            _ => panic!("expected call expression"),
        }
    }

    #[test]
    fn allows_unknown_in_logical_binary_ops() {
        let mut program = Program {
            statements: vec![
                Statement::Let {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    value: Some(Expression::Literal(Literal::Int(1))),
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                },
                Statement::Let {
                    name: "b".to_string(),
                    data_type: DataType::Unknown,
                    value: None,
                    is_constant: false,
                    is_mutable: false,
                    is_static: false,
                    visibility: Visibility::Public,
                },
                Statement::Expression(Expression::BinaryOp {
                    operator: "and".to_string(),
                    left: Box::new(Expression::Identifier(Identifier {
                        name: "a".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    right: Box::new(Expression::Identifier(Identifier {
                        name: "b".to_string(),
                        data_type: DataType::Unknown,
                        line: 0,
                        column: 0,
                    })),
                    data_type: DataType::Unknown,
                }),
            ],
        };

        check_program_types(&mut program, "").expect("type check must pass");
    }
}