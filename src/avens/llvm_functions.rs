use super::*;

impl LlvmIrGen {
    fn emit_string_locals_cleanup(&mut self) {
        let ref_body = &self.body;
        let owned_vars: Vec<(String, String)> = self
            .vars
            .iter()
            .filter(|(_, var)| var.owns_heap_string && var.ty == LlType::Ptr)
            .map(|(name, var)| (name.clone(), var.ptr.clone()))
            .collect();
        let _ = ref_body;
        for (name, ptr) in owned_vars {
            let tmp = self.tmp();
            self.body
                .push(format!("  {tmp} = load ptr, ptr {ptr}"));
            self.body
                .push(format!("  call void @rt_managed_free(ptr {tmp})"));
            if let Some(var) = self.vars.get_mut(&name) {
                var.owns_heap_string = false;
            }
        }
    }

    pub(super) fn compile_program(
        mut self,
        program: &Program,
    ) -> Result<(String, Vec<(String, String)>)> {
        // First pass: collect struct definitions
        for stmt in &program.statements {
            if let Statement::Type { name, fields, .. } = stmt {
                let mut field_types = Vec::new();
                let mut field_data_types = Vec::new();
                let mut field_indices = HashMap::new();

                for (idx, field_stmt) in fields.iter().enumerate() {
                    if let Statement::Let {
                        name: field_name,
                        data_type,
                        ..
                    } = field_stmt
                    {
                        field_types.push(self.map_type(data_type)?);
                        field_data_types.push(data_type.clone());
                        field_indices.insert(field_name.clone(), idx);
                    }
                }

                self.user_structs.insert(
                    name.clone(),
                    StructInfo {
                        fields: field_types,
                        field_data_types,
                        field_indices,
                    },
                );
            }
            // First pass: collect enum definitions
            if let Statement::Enum { name, variants, .. } = stmt {
                let mut max_payload_size = 1usize;
                let mut variant_infos = HashMap::new();
                for (idx, variant) in variants.iter().enumerate() {
                    let payload_types: Vec<LlType> = variant
                        .data_types
                        .iter()
                        .filter_map(|dt| self.map_type(dt).ok())
                        .collect();
                    max_payload_size = max_payload_size.max(payload_types.len().max(1));
                    variant_infos.insert(
                        variant.name.clone(),
                        VariantInfo {
                            tag: idx as u32,
                            payload_types,
                        },
                    );
                }
                self.user_enums.insert(
                    name.clone(),
                    EnumInfo {
                        llvm_type: format!("{{ i32, [{} x i64] }}", max_payload_size),
                        variants: variant_infos,
                    },
                );
                self.vars.insert(
                    name.clone(),
                    VarInfo {
                        ptr: format!("@enum_{}", sanitize_symbol(name)),
                        ty: LlType::Ptr,
                        data_type: DataType::EnumNamed(name.clone()),
                        owns_heap_string: false,
                        struct_name: None,
                    },
                );
            }
        }

        // Second pass: collect function signatures
        for stmt in &program.statements {
            if let Statement::ExternFunction {
                name,
                params,
                return_type,
                ..
            } = stmt
            {
                let ret = self.map_type(return_type)?;
                let param_types: Vec<LlType> = params
                    .iter()
                    .map(|(_, ty)| self.map_type(ty).unwrap_or(LlType::I64))
                    .collect();
                let sig = param_types
                    .iter()
                    .map(|ty| self.ty(ty.clone()))
                    .collect::<Vec<_>>()
                    .join(", ");
                let llvm_name = format!("@{}", sanitize_symbol(name));
                self.extern_decls.push(format!(
                    "declare {} {}({})",
                    self.ty(ret.clone()),
                    llvm_name,
                    sig
                ));
                self.user_functions.insert(
                    name.clone(),
                    FnInfo {
                        llvm_name,
                        params: param_types,
                        ret: ret.clone(),
                        returns_value: *return_type != DataType::None,
                    },
                );
            }
            if let Statement::ExternLib { name, path } = stmt {
                self.extern_libs.push((name.clone(), path.clone()));
            }
            if let Statement::Function {
                name,
                params,
                return_type,
                ..
            } = stmt
            {
                let param_types = params
                    .iter()
                    .map(|(_, ty)| self.map_type(ty))
                    .collect::<Result<Vec<_>>>()?;
                let ret = if name == "main" {
                    LlType::I64
                } else {
                    self.map_type(return_type)?
                };
                let llvm_name = self.llvm_fn_name(name);
                self.user_functions.insert(
                    name.clone(),
                    FnInfo {
                        llvm_name,
                        params: param_types,
                        ret,
                        returns_value: *return_type != DataType::None,
                    },
                );
            }
            if let Statement::Impl {
                type_name, methods, ..
            } = stmt
            {
                for method in methods {
                    if let Statement::Function {
                        name,
                        params,
                        return_type,
                        ..
                    } = method
                    {
                        let full_name = format!("{}.{}", normalize_nominal_name(type_name), name);
                        let llvm_name = self.llvm_fn_name(&full_name);
                        self.user_functions.insert(
                            full_name.clone(),
                            FnInfo {
                                llvm_name,
                                params: params
                                    .iter()
                                    .map(|(param_name, ty)| {
                                        if param_name == "self" {
                                            Ok(LlType::Ptr)
                                        } else {
                                            self.map_type(ty)
                                        }
                                    })
                                    .collect::<Result<Vec<_>>>()?,
                                ret: self.map_type(return_type)?,
                                returns_value: *return_type != DataType::None,
                            },
                        );
                    }
                }
            }
        }

        // Reset the function ID counter so the compilation pass produces the
        // same unique LLVM names as the registration pass (in the same order).
        self.next_fn_id.clear();

        for stmt in &program.statements {
            if let Statement::Function {
                name,
                params,
                body,
                return_type,
                ..
            } = stmt
            {
                let ret = if name == "main" {
                    LlType::I64
                } else {
                    self.map_type(return_type)?
                };
                let returns_value = *return_type != DataType::None;
                let fn_ir = self.compile_function_ir(name, params, body, ret, returns_value)?;
                self.functions.push(fn_ir);
            }
            if let Statement::Impl {
                type_name, methods, ..
            } = stmt
            {
                for method in methods {
                    if let Statement::Function {
                        name,
                        params,
                        body,
                        return_type,
                        ..
                    } = method
                    {
                        let full_name = format!("{}.{}", normalize_nominal_name(type_name), name);
                        let returns_value = *return_type != DataType::None;
                        let fn_ir = self.compile_function_ir(
                            &full_name,
                            params,
                            body,
                            self.map_type(return_type)?,
                            returns_value,
                        )?;
                        self.functions.push(fn_ir);
                    }
                }
            }
        }

        if let Some(Statement::Function { body, .. }) = program.statements.iter().find(
            |stmt| matches!(stmt, Statement::Function { name, params, .. } if name == "main" && params.is_empty()),
        ) {
            self.body.push("  %call_main = call i64 @mire_main()".to_string());
            if body.iter().all(|stmt| !matches!(stmt, Statement::Return(_))) {
                self.body.push("  ret i32 0".to_string());
            }
        } else {
            for stmt in &program.statements {
                self.compile_statement(stmt)?;
            }
            self.body.push("  ret i32 0".to_string());
        }

        let mut out = vec![
            "declare i32 @printf(ptr, ...)".to_string(),
            "declare i32 @fflush(ptr)".to_string(),
            "declare i32 @scanf(ptr, ...)".to_string(),
            "declare i64 @strlen(ptr)".to_string(),
            "declare i64 @clock()".to_string(),
            "declare ptr @malloc(i64)".to_string(),
            "declare void @free(ptr)".to_string(),
            "declare ptr @realloc(ptr, i64)".to_string(),
            "declare ptr @memcpy(ptr, ptr, i64)".to_string(),
            "declare i32 @memcmp(ptr, ptr, i64)".to_string(),
            "declare i32 @strcmp(ptr, ptr)".to_string(),
            "declare i32 @getpagesize()".to_string(),
            "declare i64 @getpid()".to_string(),
            "declare i64 @pal_time_mark()".to_string(),
            "declare i64 @pal_time_elapsed_ms(i64)".to_string(),
            "declare ptr @rt_time_elapsed_ms_str(i64)".to_string(),
            "declare i64 @pal_cpu_mark()".to_string(),
            "declare i64 @pal_cpu_elapsed_ms(i64)".to_string(),
            "declare ptr @rt_cpu_elapsed_ms_str(i64)".to_string(),
            "declare i64 @pal_cpu_cycles_est(i64)".to_string(),
            "declare i64 @pal_mem_process_bytes()".to_string(),
            "declare ptr @pal_mem_format(i64)".to_string(),
            "declare ptr @pal_gpu_snapshot()".to_string(),
            "declare ptr @rt_i64_to_string(i64)".to_string(),
            "declare ptr @rt_bool_to_string(i64)".to_string(),
            "declare ptr @rt_f64_to_string(double)".to_string(),
            "declare ptr @rt_string_copy(ptr)".to_string(),
            "declare ptr @rt_managed_from_cstr(ptr)".to_string(),
            "declare ptr @rt_string_concat(ptr, ptr)".to_string(),
            "declare ptr @rt_string_append_owned(ptr, ptr)".to_string(),
            "declare void @rt_managed_free(ptr)".to_string(),
            "declare ptr @rt_string_to_upper(ptr)".to_string(),
            "declare ptr @rt_string_to_lower(ptr)".to_string(),
            "declare ptr @rt_strings_replace(ptr, ptr, ptr)".to_string(),
            "declare ptr @rt_strings_split_list(ptr, ptr)".to_string(),
            "declare ptr @rt_strings_join(ptr, i64, ptr)".to_string(),
            "declare ptr @rt_strings_trim(ptr)".to_string(),
            "declare ptr @rt_strings_replace_first(ptr, ptr, ptr)".to_string(),
            "declare i64 @rt_strings_starts_with(ptr, ptr)".to_string(),
            "declare i64 @rt_strings_ends_with(ptr, ptr)".to_string(),
            "declare i64 @rt_strings_contains(ptr, ptr)".to_string(),
            "declare ptr @rt_strings_substr(ptr, i64, i64)".to_string(),
            "declare ptr @rt_strings_pad_left(ptr, i64, ptr)".to_string(),
            "declare ptr @rt_strings_pad_right(ptr, i64, ptr)".to_string(),
            "declare ptr @rt_list_create(i64, i64)".to_string(),
            "declare ptr @rt_list_push_i64(ptr, i64)".to_string(),
            "declare ptr @rt_list_push_scalar(ptr, i64, i64)".to_string(),
            "declare ptr @rt_list_push_ptr(ptr, ptr)".to_string(),
            "declare ptr @rt_list_concat(ptr, ptr)".to_string(),
            "declare i64 @rt_list_pop_i64(ptr)".to_string(),
            "declare i64 @rt_dict_get_i64(ptr, i64, i64, ptr, i64)".to_string(),
            "declare ptr @rt_dict_get_ptr(ptr, i64, i64, ptr, ptr)".to_string(),
            "declare ptr @rt_dict_set_i64(ptr, i64, i64, i64, ptr, i64)".to_string(),
            "declare ptr @rt_dict_set_ptr(ptr, i64, i64, i64, ptr, ptr)".to_string(),
            "declare ptr @rt_dict_to_string(ptr)".to_string(),
            "declare ptr @rt_dict_keys(ptr)".to_string(),
            "declare ptr @rt_dict_values(ptr)".to_string(),
            "declare ptr @rt_list_slice(ptr, i64, i64)".to_string(),
            "declare void @rt_panic(ptr)".to_string(),
            "declare ptr @fgets(ptr, i64, ptr)".to_string(),
            "declare ptr @rt_read_line(ptr)".to_string(),
            "declare i64 @atoll(ptr)".to_string(),
            "declare double @atof(ptr)".to_string(),
            "@.fmt_i64 = private unnamed_addr constant [5 x i8] c\"%ld\\0A\\00\"".to_string(),
            "@.fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"".to_string(),
            "@.fmt_float = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\"".to_string(),
            "@.fmt_f64 = private unnamed_addr constant [6 x i8] c\"%.6g\\0A\\00\"".to_string(),
            "@.fmt_bool_true = private unnamed_addr constant [5 x i8] c\"true\\00\"".to_string(),
            "@.fmt_bool_false = private unnamed_addr constant [6 x i8] c\"false\\00\"".to_string(),
            "@.fmt_i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\"".to_string(),
            // ireru remains a language builtin; library I/O uses rt_/pal_ externs.
            "@.argc = global i32 0".to_string(),
            "@.argv = global ptr null".to_string(),
            "declare ptr @rt_get_args(i32, ptr)".to_string(),
            // FS functions
            "declare i32 @pal_fs_write(ptr, ptr)".to_string(),
            "declare i32 @pal_fs_append(ptr, ptr)".to_string(),
            "declare ptr @pal_fs_read(ptr)".to_string(),
            "declare i32 @pal_fs_copy(ptr, ptr)".to_string(),
            "declare i32 @pal_fs_move(ptr, ptr)".to_string(),
            "declare i32 @pal_fs_delete(ptr)".to_string(),
            "declare i32 @pal_fs_mkdir(ptr)".to_string(),
            "declare i32 @pal_fs_rmdir(ptr)".to_string(),
            "declare i64 @pal_fs_exists(ptr)".to_string(),
            "declare i64 @pal_fs_is_dir(ptr)".to_string(),
            "declare i64 @pal_fs_is_file(ptr)".to_string(),
            "declare i64 @pal_fs_size(ptr)".to_string(),
            "declare ptr @pal_fs_list(ptr)".to_string(),
            "declare ptr @pal_fs_join(ptr, ptr)".to_string(),
            "declare ptr @pal_fs_dir(ptr)".to_string(),
            "declare ptr @pal_fs_name(ptr)".to_string(),
            "declare ptr @pal_fs_ext(ptr)".to_string(),
            // TLS functions
            "declare i64 @pal_tls_connect(ptr, i64)".to_string(),
            "declare i32 @pal_tls_send(i64, ptr)".to_string(),
            "declare ptr @pal_tls_recv(i64, i64)".to_string(),
            "declare i32 @pal_tls_close(i64)".to_string(),
            // WS functions
            "declare i64 @pal_ws_connect(ptr, i64, ptr)".to_string(),
            "declare i32 @pal_ws_send_text(i64, ptr)".to_string(),
            "declare ptr @pal_ws_recv(i64, i64)".to_string(),
            "declare i32 @pal_ws_close(i64)".to_string(),
            "declare i64 @pal_wss_connect(ptr, i64, ptr)".to_string(),
            "declare i32 @pal_wss_send_text(i64, ptr)".to_string(),
            "declare ptr @pal_wss_recv(i64, i64)".to_string(),
            "declare i32 @pal_wss_close(i64)".to_string(),
            // NET functions
            "declare i64 @pal_net_connect(ptr, i64)".to_string(),
            "declare i64 @pal_net_connect_timeout(ptr, i64, i64)".to_string(),
            "declare ptr @pal_net_recv(i64, i64)".to_string(),
            "declare i32 @pal_net_send(i64, ptr)".to_string(),
            "declare i32 @pal_net_close(i64)".to_string(),
            "declare i64 @pal_net_poll(i64, i64)".to_string(),
            "declare i32 @pal_net_set_nonblock(i64, i32)".to_string(),
            "declare ptr @pal_net_resolve(ptr)".to_string(),
            // PROC functions
            "declare ptr @pal_proc_run(ptr)".to_string(),
            "declare ptr @pal_proc_exec(ptr)".to_string(),
            "declare ptr @pal_proc_shell(ptr)".to_string(),
            "declare i64 @pal_proc_spawn(ptr)".to_string(),
            "declare i64 @pal_proc_wait(i64)".to_string(),
            "declare i32 @pal_proc_kill(i64)".to_string(),
            "declare void @pal_proc_exit(i64)".to_string(),
            "declare i64 @pal_proc_exists(i64)".to_string(),
            "declare i64 @abs(i64)".to_string(),
            "declare double @rt_math_sqrt(double)".to_string(),
            "declare double @rt_math_pow(double, double)".to_string(),
            "declare i64 @rt_math_round(double)".to_string(),
            "declare i64 @rt_math_floor(double)".to_string(),
            "declare i64 @rt_math_ceil(double)".to_string(),
            // ENV functions
            "declare ptr @pal_env_get(ptr)".to_string(),
            "declare i32 @pal_env_set(ptr, ptr)".to_string(),
            "declare ptr @pal_env_cwd()".to_string(),
            "declare ptr @pal_env_all()".to_string(),
        ];
        out.extend(self.strings);
        // Deduplicate extern declarations by function name.
        // Hard-coded declarations (already in `out`) take priority.
        let extra_externs: Vec<String> = {
            fn extract_fn_name(decl: &str) -> Option<&str> {
                let start = decl.find('@')?;
                let end = decl[start..].find('(')?;
                Some(&decl[start + 1..start + end])
            }
            let mut seen: std::collections::HashSet<&str> =
                out.iter().filter_map(|s| extract_fn_name(s)).collect();
            self.extern_decls
                .iter()
                .filter(|decl| extract_fn_name(decl).is_none_or(|name| seen.insert(name)))
                .cloned()
                .collect()
        };
        out.extend(extra_externs);
        out.push(String::new());
        let has_functions = !self.functions.is_empty();
        out.extend(self.functions);
        if has_functions {
            out.push(String::new());
        }
        out.push("define i32 @main(i32 %argc, i8** %argv) {".to_string());
        out.push("entry:".to_string());
        out.push("  store i32 %argc, ptr @.argc".to_string());
        out.push("  store i8** %argv, ptr @.argv".to_string());
        out.extend(self.entry_allocas);
        out.extend(self.body);
        out.push("}".to_string());
        out.push(String::new());
        Ok((out.join("\n"), self.extern_libs))
    }

    pub(super) fn compile_statement(&mut self, stmt: &Statement) -> Result<()> {
        let (line, column) = Self::statement_location(stmt);
        self.current_line = line;
        self.current_column = column;
        let result = match stmt {
            Statement::Load { .. } => Ok(()),
            Statement::Function { .. } => Ok(()),
            Statement::Let {
                name,
                data_type,
                value,
                ..
            } => {
                let ll_ty = self.map_type(data_type)?;
                let ptr = self.tmp();
                self.entry_allocas
                    .push(format!("  {ptr} = alloca {}", self.ty(ll_ty.clone())));
                self.vars.insert(
                    name.clone(),
                    VarInfo {
                        ptr: ptr.clone(),
                        ty: ll_ty.clone(),
                        data_type: data_type.clone(),
                        owns_heap_string: false,
                        struct_name: value
                            .as_ref()
                            .and_then(|expr| self.struct_name_from_expr(expr)),
                    },
                );
                let init = if let Some(expr) = value {
                    self.compile_expr(expr)?
                } else {
                    self.default_value(ll_ty.clone())
                };
                self.store_variable(name, &ptr, ll_ty, data_type.clone(), init)?;
                if *data_type == DataType::Function {
                    if let Some(expr) = value {
                        if let Some(alias) = self.function_name_from_expr(expr) {
                            self.function_aliases.insert(name.clone(), alias);
                        } else {
                            self.function_aliases.remove(name);
                        }
                        if let Some(sig) = self.function_signature_from_expr(expr) {
                            self.function_value_signatures.insert(name.clone(), sig);
                        } else {
                            self.function_value_signatures.remove(name);
                        }
                    } else {
                        self.function_aliases.remove(name);
                        self.function_value_signatures.remove(name);
                    }
                } else {
                    self.function_aliases.remove(name);
                    self.function_value_signatures.remove(name);
                }
                Ok(())
            }
            Statement::Assignment { target, value, .. } => match target {
                AssignmentTarget::Field(path) => self.compile_field_assignment(path, value),
                AssignmentTarget::Index { target, index } => {
                    self.compile_index_assignment(target, index, value)
                }
                AssignmentTarget::Variable(name) => {
                    let var = self.vars.get(name).cloned().ok_or_else(|| {
                        MireError::new(ErrorKind::Runtime {
                            message: format!("Avenys does not know variable '{}'", name),
                        })
                    })?;
                    if self.try_compile_in_place_string_append(name, &var, value)? {
                        return Ok(());
                    }
                    let compiled = self.compile_expr(value)?;
                    self.store_variable(name, &var.ptr, var.ty, var.data_type.clone(), compiled)?;
                    let struct_name = self.struct_name_from_expr(value);
                    if let Some(slot) = self.vars.get_mut(name) {
                        slot.struct_name = struct_name;
                    }
                    if var.data_type == DataType::Function {
                        if let Some(alias) = self.function_name_from_expr(value) {
                            self.function_aliases.insert(name.clone(), alias);
                        } else {
                            self.function_aliases.remove(name);
                        }
                        if let Some(sig) = self.function_signature_from_expr(value) {
                            self.function_value_signatures.insert(name.clone(), sig);
                        } else {
                            self.function_value_signatures.remove(name);
                        }
                    } else {
                        self.function_aliases.remove(name);
                        self.function_value_signatures.remove(name);
                    }
                    Ok(())
                }
            },
            Statement::While { condition, body } => {
                let cond_label = self.label("while_cond");
                let body_label = self.label("while_body");
                let end_label = self.label("while_end");
                self.body.push(format!("  br label %{cond_label}"));
                self.body.push(format!("{cond_label}:"));
                let cond_val = self.compile_expr(condition)?;
                let cond = self.cast_to_i1(cond_val)?;
                self.body.push(format!(
                    "  br i1 {}, label %{body_label}, label %{end_label}",
                    cond.repr
                ));
                self.body.push(format!("{body_label}:"));
                self.loop_stack.push(LoopLabels {
                    break_label: end_label.clone(),
                    continue_label: cond_label.clone(),
                });
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                self.loop_stack.pop();
                self.body.push(format!("  br label %{cond_label}"));
                self.body.push(format!("{end_label}:"));
                Ok(())
            }
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => self.compile_for_range(variable, index.as_deref(), iterable, body),
            Statement::Find {
                variable,
                iterable,
                body,
            } => self.compile_for_range(variable, None, iterable, body),
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let then_label = self.label("if_then");
                let else_label = self.label("if_else");
                let end_label = self.label("if_end");
                let cond_val = self.compile_expr(condition)?;
                let cond = self.cast_to_i1(cond_val)?;
                self.body.push(format!(
                    "  br i1 {}, label %{then_label}, label %{else_label}",
                    cond.repr
                ));
                self.body.push(format!("{then_label}:"));
                for stmt in then_branch {
                    self.compile_statement(stmt)?;
                }
                self.body.push(format!("  br label %{end_label}"));
                self.body.push(format!("{else_label}:"));
                if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        self.compile_statement(stmt)?;
                    }
                }
                self.body.push(format!("  br label %{end_label}"));
                self.body.push(format!("{end_label}:"));
                Ok(())
            }
            Statement::Match {
                value,
                cases,
                default,
            } => self.compile_match_statement(value, cases, default),
            Statement::Expression(Expression::Call { name, args, .. }) if name == "__do_while" => {
                self.compile_do_while(args)
            }
            Statement::Expression(Expression::Call { name, args, .. }) if name == "dasu" => {
                for arg in args {
                    self.emit_dasu_expr(arg)?;
                }
                Ok(())
            }
            Statement::Expression(expr) => {
                let _ = self.compile_expr(expr)?;
                Ok(())
            }
            Statement::Break => {
                let labels = self.loop_stack.last().cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: "Avenys found `break` outside of a loop".to_string(),
                    })
                })?;
                self.body
                    .push(format!("  br label %{}", labels.break_label));
                Ok(())
            }
            Statement::Continue => {
                let labels = self.loop_stack.last().cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: "Avenys found `continue` outside of a loop".to_string(),
                    })
                })?;
                self.body
                    .push(format!("  br label %{}", labels.continue_label));
                Ok(())
            }
            Statement::Return(expr) => {
                self.emit_string_locals_cleanup();
                let ret_ty = self.current_return.clone();
                let value = if let Some(expr) = expr {
                    self.compile_expr(expr)?
                } else {
                    self.default_value(ret_ty.clone())
                };
                let ret = self.cast_to_type(value, ret_ty.clone())?;
                self.body
                    .push(format!("  ret {} {}", self.ty(ret_ty), ret.repr));
                Ok(())
            }
            Statement::Unsafe { body } => {
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                Ok(())
            }
            Statement::ExternLib { .. } | Statement::ExternFunction { .. } => Ok(()),
            Statement::Asm { instructions } => {
                for (template, operand) in instructions {
                    let operand_expr = self.compile_expr(operand)?;
                    let operand_val = self.cast_to_i64(operand_expr)?;
                    self.body.push(format!(
                        "  call void asm sideeffect \"{}\", \"r\"(i64 {})",
                        template.replace('"', "\\22"),
                        operand_val.repr
                    ));
                }
                Ok(())
            }
            // Frontend-only declarations/analysis statements: currently no direct IR emission.
            Statement::Type { .. }
            | Statement::Skill { .. }
            | Statement::Impl { .. }
            | Statement::Enum { .. }
            | Statement::Query { .. } => Err(MireError::new(ErrorKind::Backend {
                message: "Avenys statement is parsed/typechecked but not lowered in backend yet"
                    .to_string(),
            })),
            Statement::Drop { value } => {
                let dropped = self.compile_expr(value)?;
                if dropped.ty == LlType::Ptr {
                    let dropped_ty = self.expression_data_type(value);
                    if dropped_ty == DataType::Str {
                        self.body.push(format!(
                            "  call void @rt_managed_free(ptr {})",
                            dropped.repr
                        ));
                    } else {
                        self.body
                            .push(format!("  call void @free(ptr {})", dropped.repr));
                    }
                }
                Ok(())
            }
            Statement::New { value, .. } | Statement::Own { value, .. } => {
                if let Some(value) = value {
                    let _ = self.compile_expr(value)?;
                }
                Ok(())
            }
            Statement::Move { target, value } => {
                let var = self.vars.get(target).cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Avenys does not know variable '{}'", target),
                    })
                })?;
                let compiled = self.compile_expr(value)?;
                self.store_variable(target, &var.ptr, var.ty, var.data_type, compiled)?;
                Ok(())
            }
            Statement::Module { .. } => Ok(()),
        };
        result.map_err(|err| self.attach_context(err))
    }

    pub(super) fn compile_expr(&mut self, expr: &Expression) -> Result<LlValue> {
        let (line, column) = Self::expression_location(expr);
        self.current_line = line;
        self.current_column = column;
        let result = match expr {
            Expression::Literal(Literal::Int(value)) => Ok(LlValue {
                ty: LlType::I64,
                repr: value.to_string(),
                owned: false,
            }),
            Expression::Literal(Literal::Float(value)) => Ok(LlValue {
                ty: LlType::F64,
                repr: format!("{value:?}"),
                owned: false,
            }),
            Expression::Literal(Literal::Char(value)) => Ok(LlValue {
                ty: LlType::I64,
                repr: (*value as i64).to_string(),
                owned: false,
            }),
            Expression::Literal(Literal::Bool(value)) => Ok(LlValue {
                ty: LlType::I1,
                repr: if *value {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
                owned: false,
            }),
            Expression::Literal(Literal::Str(value)) => Ok(self.string_value(value)),
            Expression::Literal(Literal::List(elements)) => {
                self.compile_list_literal(elements, &DataType::Unknown)
            }
            Expression::Literal(Literal::Dict(entries)) => {
                let lowered_entries: Vec<(Expression, Expression)> = entries
                    .iter()
                    .map(|((key, value), _)| (key.clone(), value.clone()))
                    .collect();
                self.compile_dict_literal(&lowered_entries)
            }
            Expression::Literal(Literal::Tuple(elements)) => {
                self.compile_list_literal(elements, &DataType::Unknown)
            }
            Expression::Literal(Literal::None) => Ok(LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            }),
            Expression::Reference { expr, .. } => self.compile_reference_expr(expr),
            Expression::Dereference { expr, data_type } => {
                self.compile_dereference_expr(expr, data_type)
            }
            Expression::Identifier(Identifier { name, .. }) => {
                if let Some(var) = self.vars.get(name).cloned() {
                    let tmp = self.tmp();
                    let var_ty = var.ty.clone();
                    self.body.push(format!(
                        "  {tmp} = load {}, ptr {}",
                        self.ty(var_ty.clone()),
                        var.ptr
                    ));
                    return Ok(LlValue {
                        ty: var_ty,
                        repr: tmp,
                        owned: var.owns_heap_string,
                    });
                }
                if let Some(fn_info) = self.user_functions.get(name).cloned().or_else(|| {
                    strip_root_namespace(name)
                        .and_then(|alias| self.user_functions.get(&alias).cloned())
                }) {
                    let tmp = self.tmp();
                    let signature = format!(
                        "{} ({})*",
                        self.ty(fn_info.ret.clone()),
                        fn_info
                            .params
                            .iter()
                            .map(|ty| self.ty(ty.clone()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    self.body.push(format!(
                        "  {tmp} = bitcast {signature} {} to ptr",
                        fn_info.llvm_name
                    ));
                    return Ok(LlValue {
                        ty: LlType::Ptr,
                        repr: tmp,
                        owned: false,
                    });
                }
                Err(MireError::new(ErrorKind::Runtime {
                    message: format!("Avenys unknown identifier '{}'", name),
                }))
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } if operator == "+" && *data_type == DataType::Str => {
                if matches!(&**left, Expression::Literal(Literal::Str(value)) if value.is_empty()) {
                    return self.compile_expr(right);
                }
                if matches!(&**right, Expression::Literal(Literal::Str(value)) if value.is_empty())
                {
                    return self.compile_expr(left);
                }
                if let (
                    Expression::Literal(Literal::Str(lhs)),
                    Expression::Literal(Literal::Str(rhs)),
                ) = (&**left, &**right)
                {
                    return Ok(self.string_value(&format!("{lhs}{rhs}")));
                }
                let lhs = self.compile_expr(left)?;
                let rhs = self.compile_expr(right)?;
                Ok(self.concat_values(lhs, rhs))
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
                ..
            } => {
                if operator == "&&" || operator == "||" {
                    return self.compile_logical_short_circuit(operator, left, right, data_type);
                }

                let lhs = self.compile_expr(left)?;
                let rhs = self.compile_expr(right)?;

                let left_is_list = matches!(data_type, DataType::Vector { .. } | DataType::List);
                let right_is_list = matches!(data_type, DataType::Vector { .. } | DataType::List);

                if operator == "+" && left_is_list && right_is_list {
                    let result = self.tmp();
                    self.body.push(format!(
                        "  {result} = call ptr @rt_list_concat(ptr {}, ptr {})",
                        lhs.repr, rhs.repr
                    ));
                    return Ok(LlValue {
                        ty: LlType::Ptr,
                        repr: result,
                        owned: true,
                    });
                }

                self.compile_binary(operator, lhs, rhs, data_type)
            }
            Expression::UnaryOp {
                operator, operand, ..
            } => {
                let value = self.compile_expr(operand)?;
                self.compile_unary(operator, value)
            }
            Expression::Call { name, args, .. } if name == "str" => {
                let value = self.compile_expr(&args[0])?;
                let arg_type = self.expression_data_type(&args[0]);
                match arg_type {
                    DataType::Str => Ok(value),
                    DataType::Dict | DataType::Map { .. } => {
                        let tmp = self.tmp();
                        self.body.push(format!(
                            "  {tmp} = call ptr @rt_dict_to_string(ptr {})",
                            value.repr
                        ));
                        Ok(LlValue {
                            ty: LlType::Ptr,
                            repr: tmp,
                            owned: true,
                        })
                    }
                    DataType::Bool => {
                        let i64_value = self.cast_to_i64(value)?;
                        let tmp = self.tmp();
                        self.body.push(format!(
                            "  {tmp} = call ptr @rt_bool_to_string(i64 {})",
                            i64_value.repr
                        ));
                        Ok(LlValue {
                            ty: LlType::Ptr,
                            repr: tmp,
                            owned: true,
                        })
                    }
                    DataType::F64 => {
                        let tmp = self.tmp();
                        self.body.push(format!(
                            "  {tmp} = call ptr @rt_f64_to_string(double {})",
                            value.repr
                        ));
                        Ok(LlValue {
                            ty: LlType::Ptr,
                            repr: tmp,
                            owned: true,
                        })
                    }
                    _ => match value.ty {
                        LlType::Ptr => Ok(value),
                        LlType::I64 => {
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @rt_i64_to_string(i64 {})",
                                value.repr
                            ));
                            Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            })
                        }
                        LlType::I1 => {
                            let i64_value = self.cast_to_i64(value)?;
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @rt_bool_to_string(i64 {})",
                                i64_value.repr
                            ));
                            Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            })
                        }
                        LlType::F64 => {
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @rt_f64_to_string(double {})",
                                value.repr
                            ));
                            Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            })
                        }
                        LlType::I8 => {
                            let i64_value = self.cast_to_i64(value)?;
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @rt_i64_to_string(i64 {})",
                                i64_value.repr
                            ));
                            Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            })
                        }
                        LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                            message: "Cannot convert struct to string".to_string(),
                        })),
                    },
                }
            }
            Expression::Call { name, args, .. } if name == "len" => self.compile_len(args),
            Expression::Call { name, args, .. } if name == "dasu" => {
                for arg in args {
                    self.emit_dasu_expr(arg)?;
                }
                Ok(self.string_value(""))
            }
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } if name == "ireru" => self.compile_input_expr(args, data_type),
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } if name == "__if_expr" => self.compile_if_expr(args, data_type),
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } if name == "call" => {
                if args.is_empty() {
                    return Err(MireError::new(ErrorKind::Runtime {
                        message: "Avenys call(...) expects at least callback argument".to_string(),
                    }));
                }
                match &args[0] {
                    Expression::Identifier(ident) => {
                        let callback_name = self
                            .function_aliases
                            .get(&ident.name)
                            .cloned()
                            .unwrap_or_else(|| ident.name.clone());
                        let fn_info =
                            self.user_functions
                                .get(&callback_name)
                                .cloned()
                                .or_else(|| {
                                    strip_root_namespace(&callback_name)
                                        .and_then(|alias| self.user_functions.get(&alias).cloned())
                                });
                        if fn_info.is_none() {
                            let dynamic_sig =
                                self.function_value_signatures.get(&ident.name).cloned();
                            if let Some(sig) = dynamic_sig {
                                if sig.params.len() != args.len() - 1 {
                                    return Err(MireError::new(ErrorKind::Runtime {
                                        message: format!(
                                            "Avenys callback '{}' expects {} args, got {}",
                                            ident.name,
                                            sig.params.len(),
                                            args.len() - 1
                                        ),
                                    }));
                                }
                            } else {
                                return Err(MireError::new(ErrorKind::Backend {
                                    message: format!(
                                        "Avenys call(...) callback '{}' has no inferable signature; reject dynamic lowering",
                                        ident.name
                                    ),
                                }));
                            }
                            if matches!(data_type, DataType::Unknown | DataType::Function) {
                                return Err(MireError::new(ErrorKind::Backend {
                                    message: format!(
                                        "Avenys call(...) callback '{}' produced unresolved return type; dynamic lowering rejected",
                                        ident.name
                                    ),
                                }));
                            }
                            let callback_ptr = self.compile_expr(&args[0])?;
                            if callback_ptr.ty != LlType::Ptr {
                                return Err(MireError::new(ErrorKind::Backend {
                                    message: format!(
                                        "Avenys dynamic callback '{}' is not a function pointer",
                                        ident.name
                                    ),
                                }));
                            }
                            let mut rendered_args = Vec::with_capacity(args.len() - 1);
                            let mut arg_tys = Vec::with_capacity(args.len() - 1);
                            for arg_expr in &args[1..] {
                                let value = self.compile_expr(arg_expr)?;
                                let (ty_name, rendered) = match value.ty {
                                    LlType::I64 => ("i64".to_string(), value.repr),
                                    LlType::I8 => {
                                        let widened = self.cast_to_i64(value)?;
                                        ("i64".to_string(), widened.repr)
                                    }
                                    LlType::I1 => {
                                        let widened = self.cast_to_i64(value)?;
                                        ("i64".to_string(), widened.repr)
                                    }
                                    LlType::F64 => ("double".to_string(), value.repr),
                                    LlType::Ptr => ("ptr".to_string(), value.repr),
                                    LlType::Struct(_) => {
                                        return Err(MireError::new(ErrorKind::Backend {
                                            message: "Cannot pass struct to callback".to_string(),
                                        }));
                                    }
                                };
                                arg_tys.push(ty_name.clone());
                                rendered_args.push(format!("{ty_name} {rendered}"));
                            }
                            let ret_ty = self.map_type(data_type).unwrap_or(LlType::I64);
                            let ret_ir = self.ty(ret_ty.clone());
                            let sig = format!("{ret_ir} ({})*", arg_tys.join(", "));
                            let callable = self.tmp();
                            self.body.push(format!(
                                "  {callable} = bitcast ptr {} to {}",
                                callback_ptr.repr, sig
                            ));
                            let out = self.tmp();
                            self.body.push(format!(
                                "  {out} = call {ret_ir} {callable}({})",
                                rendered_args.join(", ")
                            ));
                            return Ok(LlValue {
                                ty: ret_ty,
                                repr: out,
                                owned: false,
                            });
                        }
                        let fn_info = fn_info.expect("checked is_some");
                        if fn_info.params.len() != args.len() - 1 {
                            return Err(MireError::new(ErrorKind::Runtime {
                                message: format!(
                                    "Avenys callback '{}' expects {} args, got {}",
                                    callback_name,
                                    fn_info.params.len(),
                                    args.len() - 1
                                ),
                            }));
                        }
                        let mut rendered_args = Vec::with_capacity(fn_info.params.len());
                        for (arg_expr, expected_ty) in args[1..].iter().zip(fn_info.params.iter()) {
                            let value = self.compile_expr(arg_expr)?;
                            let casted = match expected_ty {
                                LlType::I64 => self.cast_to_i64(value)?,
                                LlType::I1 => self.cast_to_i1(value)?,
                                LlType::I8 => value,
                                LlType::F64 => self.cast_to_f64(value)?,
                                LlType::Ptr if value.ty == LlType::Ptr => value,
                                LlType::Ptr => {
                                    return Err(MireError::new(ErrorKind::Runtime {
                                        message: format!(
                                            "Avenys cannot cast callback argument for '{}'",
                                            callback_name
                                        ),
                                    }));
                                }
                                LlType::Struct(_) => value,
                            };
                            rendered_args.push(format!(
                                "{} {}",
                                self.ty(expected_ty.clone()),
                                casted.repr
                            ));
                        }
                        let tmp = self.tmp();
                        self.body.push(format!(
                            "  {tmp} = call {} {}({})",
                            self.ty(fn_info.ret.clone()),
                            fn_info.llvm_name,
                            rendered_args.join(", ")
                        ));
                        Ok(LlValue {
                            ty: fn_info.ret,
                            repr: tmp,
                            owned: false,
                        })
                    }
                    Expression::Closure {
                        params,
                        body,
                        return_type,
                        capture: _,
                    } => {
                        if params.len() != args.len() - 1 {
                            return Err(MireError::new(ErrorKind::Runtime {
                                message: format!(
                                    "Avenys call(...) closure expects {} args, got {}",
                                    params.len(),
                                    args.len() - 1
                                ),
                            }));
                        }
                        let mut bound_values = Vec::with_capacity(params.len());
                        for arg in &args[1..] {
                            bound_values.push(self.compile_expr(arg)?);
                        }
                        self.compile_bound_closure(params, &bound_values, body, return_type)
                    }
                    callback_expr => {
                        if matches!(data_type, DataType::Unknown | DataType::Function) {
                            return Err(MireError::new(ErrorKind::Backend {
                                message:
                                    "Avenys call(...) callback expression has unresolved return type; dynamic lowering rejected"
                                        .to_string(),
                            }));
                        }
                        let callback_ptr = self.compile_expr(callback_expr)?;
                        if callback_ptr.ty != LlType::Ptr {
                            return Err(MireError::new(ErrorKind::Backend {
                                message:
                                    "Avenys call(...) dynamic callback must be a function pointer"
                                        .to_string(),
                            }));
                        }
                        let mut rendered_args = Vec::with_capacity(args.len() - 1);
                        let mut arg_tys = Vec::with_capacity(args.len() - 1);
                        for arg_expr in &args[1..] {
                            let value = self.compile_expr(arg_expr)?;
                            let (ty_name, rendered) = match value.ty {
                                LlType::I64 => ("i64".to_string(), value.repr),
                                LlType::I8 => {
                                    let widened = self.cast_to_i64(value)?;
                                    ("i64".to_string(), widened.repr)
                                }
                                LlType::I1 => {
                                    let widened = self.cast_to_i64(value)?;
                                    ("i64".to_string(), widened.repr)
                                }
                                LlType::F64 => ("double".to_string(), value.repr),
                                LlType::Ptr => ("ptr".to_string(), value.repr),
                                LlType::Struct(_) => {
                                    return Err(MireError::new(ErrorKind::Backend {
                                        message: "Cannot pass struct to callback".to_string(),
                                    }));
                                }
                            };
                            arg_tys.push(ty_name.clone());
                            rendered_args.push(format!("{ty_name} {rendered}"));
                        }
                        let ret_ty = self.map_type(data_type).unwrap_or(LlType::I64);
                        let ret_ir = self.ty(ret_ty.clone());
                        let sig = format!("{ret_ir} ({})*", arg_tys.join(", "));
                        let callable = self.tmp();
                        self.body.push(format!(
                            "  {callable} = bitcast ptr {} to {}",
                            callback_ptr.repr, sig
                        ));
                        let out = self.tmp();
                        self.body.push(format!(
                            "  {out} = call {ret_ir} {callable}({})",
                            rendered_args.join(", ")
                        ));
                        Ok(LlValue {
                            ty: ret_ty,
                            repr: out,
                            owned: false,
                        })
                    }
                }
            }
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } if name == "new::" || name == "move::" => {
                if let Some(first) = args.first() {
                    self.compile_expr(first)
                } else {
                    let ll_ty = self.map_type(data_type)?;
                    Ok(self.default_value(ll_ty))
                }
            }
            Expression::Call { name, args, .. } if name == "own::" => {
                let source = if let Some(first) = args.first() {
                    self.compile_expr(first)?
                } else {
                    LlValue {
                        ty: LlType::I64,
                        repr: "0".to_string(),
                        owned: false,
                    }
                };
                self.heap_box_value(source)
            }
            Expression::Call { name, args, .. } if name == "drop::" => {
                for arg in args {
                    let dropped = self.compile_expr(arg)?;
                    if dropped.ty == LlType::Ptr {
                        let dropped_ty = self.expression_data_type(arg);
                        if dropped_ty == DataType::Str {
                            self.body.push(format!(
                                "  call void @rt_managed_free(ptr {})",
                                dropped.repr
                            ));
                        } else {
                            self.body
                                .push(format!("  call void @free(ptr {})", dropped.repr));
                        }
                    }
                }
                Ok(self.default_value(LlType::I64))
            }
            Expression::Try { expr, data_type } => {
                let result = self.compile_expr(expr)?;
                let tag = self.tmp();
                self.body.push(format!(
                    "  {tag} = extractvalue {{ i8, ptr }} {}, 0",
                    result.repr
                ));
                let is_ok = self.tmp();
                self.body.push(format!("  {is_ok} = icmp eq i8 {tag}, 0"));
                let ok_label = self.label("try_ok");
                let err_label = self.label("try_err");
                self.body.push(format!(
                    "  br i1 {is_ok}, label %{ok_label}, label %{err_label}"
                ));
                self.body.push(format!("{err_label}:"));
                self.body.push(format!(
                    "  ret {} {}",
                    self.ty(self.current_return.clone()),
                    result.repr
                ));
                self.body.push(format!("{ok_label}:"));
                let payload_ptr = self.tmp();
                self.body.push(format!(
                    "  {payload_ptr} = extractvalue {{ i8, ptr }} {}, 1",
                    result.repr
                ));
                let payload_ty = self.map_type(data_type)?;
                let loaded = self.tmp();
                self.body.push(format!(
                    "  {loaded} = load {}, ptr {}",
                    self.ty(payload_ty.clone()),
                    payload_ptr
                ));
                Ok(LlValue {
                    ty: payload_ty,
                    repr: loaded,
                    owned: false,
                })
            }
            Expression::Ok { value, .. } => {
                let val = self.compile_expr(value)?;
                let boxed = self.heap_box_value(val)?;
                let tmp = self.tmp();
                self.body.push(format!(
                    "  {tmp} = insertvalue {{ i8, ptr }} zeroinitializer, i8 0, 0"
                ));
                let tmp2 = self.tmp();
                self.body.push(format!(
                    "  {tmp2} = insertvalue {{ i8, ptr }} {tmp}, ptr {}, 1",
                    boxed.repr
                ));
                Ok(LlValue {
                    ty: LlType::Struct(vec![LlType::I8, LlType::Ptr]),
                    repr: tmp2,
                    owned: true,
                })
            }
            Expression::Err { value, .. } => {
                let val = self.compile_expr(value)?;
                let boxed = self.heap_box_value(val)?;
                let tmp = self.tmp();
                self.body.push(format!(
                    "  {tmp} = insertvalue {{ i8, ptr }} zeroinitializer, i8 1, 0"
                ));
                let tmp2 = self.tmp();
                self.body.push(format!(
                    "  {tmp2} = insertvalue {{ i8, ptr }} {tmp}, ptr {}, 1",
                    boxed.repr
                ));
                Ok(LlValue {
                    ty: LlType::Struct(vec![LlType::I8, LlType::Ptr]),
                    repr: tmp2,
                    owned: true,
                })
            }
            Expression::Match {
                value,
                cases,
                default,
                data_type,
            } => self.compile_match_expr(value, cases, default, data_type),
            Expression::List {
                elements,
                element_type,
                ..
            } => self.compile_list_literal(elements, element_type),
            Expression::Dict { entries, .. } => self.compile_dict_literal(entries),
            Expression::Index {
                target,
                index,
                data_type,
            } => {
                let target_val = self.compile_expr(target)?;
                let index_val = self.compile_expr(index)?;
                let target_type = self.expression_data_type(target);
                let effective_type =
                    if matches!(target_type, DataType::Vector { dynamic: false, .. }) {
                        target_type.clone()
                    } else {
                        target_type
                    };
                self.compile_index(target_val, index_val, &effective_type, data_type)
            }
            Expression::MemberAccess { target, member, .. } => {
                self.compile_member_access(target, member)
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                ..
            } => self.compile_enum_variant_path(enum_name, variant_name),
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                ..
            } => self.compile_enum_variant(enum_name, variant_name, payloads),
            Expression::Call { name, args, .. } if name == "lists.push" => {
                self.compile_lists_push(args)
            }
            Expression::Call { name, args, .. } if name == "lists.slice" => {
                self.compile_lists_slice(args)
            }
            Expression::Call { name, args, .. } if name == "lists.len" => {
                self.compile_list_len(args)
            }
            Expression::Call { name, args, .. } if name == "lists.get" => {
                self.compile_list_get(args)
            }
            Expression::Call { name, args, .. } if name == "lists.first" => {
                let list_val = self.compile_expr(&args[0])?;
                let list_type = self.expression_data_type(&args[0]);
                let elem_type = match &list_type {
                    DataType::Vector { element_type, .. }
                    | DataType::Array { element_type, .. } => *element_type.clone(),
                    DataType::Slice { element_type } => *element_type.clone(),
                    _ => DataType::I64,
                };
                let zero = LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                };
                self.compile_index(list_val, zero, &list_type, &elem_type)
            }
            Expression::Call { name, args, .. } if name == "lists.last" => {
                let list_val = self.compile_expr(&args[0])?;
                let list_type = self.expression_data_type(&args[0]);
                let elem_type = match &list_type {
                    DataType::Vector { element_type, .. }
                    | DataType::Array { element_type, .. } => *element_type.clone(),
                    DataType::Slice { element_type } => *element_type.clone(),
                    _ => DataType::I64,
                };
                let len = self.compile_list_len_value(list_val.clone())?;
                let last_idx = self.tmp();
                self.body
                    .push(format!("  {last_idx} = sub i64 {}, 1", len.repr));
                let index = LlValue {
                    ty: LlType::I64,
                    repr: last_idx,
                    owned: false,
                };
                self.compile_index(list_val, index, &list_type, &elem_type)
            }
            Expression::Call { name, args, .. } if name == "lists.is_empty" => {
                let list_val = self.compile_expr(&args[0])?;
                let len = self.compile_list_len_value(list_val)?;
                let result = self.tmp();
                self.body
                    .push(format!("  {result} = icmp eq i64 {}, 0", len.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            Expression::Call { name, args, .. } if name == "lists.append" => {
                self.compile_lists_push(args)
            }
            Expression::Call { name, args, .. } if name == "lists.pop" => {
                self.compile_list_pop(args)
            }
            Expression::Call { name, args, .. } if name == "pop" => self.compile_list_pop(args),
            Expression::Call { name, args, .. } if name == "dicts.get" => {
                self.compile_dict_get(args)
            }
            Expression::Call { name, args, .. } if name == "dicts.set" => {
                self.compile_dict_set(args)
            }
            Expression::Call { name, args, .. } if name == "contains" => {
                self.compile_contains(args)
            }
            Expression::Call { name, args, .. } if name == "strings.contains" => {
                self.compile_contains(args)
            }
            Expression::Call { name, args, .. } if name == "dicts.keys" => {
                self.compile_dict_keys(args)
            }
            Expression::Call { name, args, .. } if name == "dicts.values" => {
                self.compile_dict_values(args)
            }
            Expression::Call { name, args, .. } if name == "float" => self.compile_float(args),
            Expression::Call { name, args, .. } if name == "int" => self.compile_int(args),
            Expression::Call { name, args, .. } if name == "bool" => self.compile_bool(args),
            Expression::Call { name, args, .. } if name == "concat" => self.compile_concat(args),
            Expression::Call { name, args, .. } if name == "strings.concat" => {
                self.compile_concat(args)
            }
            Expression::Call { name, args, .. } if name == "strings.len" => self.compile_len(args),
            Expression::Call { name, args, .. } if name == "strings.replace" => {
                self.compile_replace(args)
            }
            Expression::Call { name, args, .. } if name == "strings.substr" => {
                self.compile_substr(args)
            }
            Expression::Call { name, args, .. } if name == "strings.split" => {
                self.compile_split(args)
            }
            Expression::Call { name, args, .. } if name == "strings.join" => {
                self.compile_join(args)
            }
            Expression::Call { name, args, .. } if name == "strings.to_upper" => {
                self.compile_to_upper(args)
            }
            Expression::Call { name, args, .. } if name == "strings.to_lower" => {
                self.compile_to_lower(args)
            }
            Expression::Call { name, args, .. } if name == "strings.trim" => {
                self.compile_trim(args)
            }
            Expression::Call { name, args, .. }
                if name == "strings.strip"
                    || name == "strings.ltrim"
                    || name == "strings.rtrim" =>
            {
                self.compile_trim(args)
            }
            Expression::Call { name, args, .. } if name == "strings.to_string" => {
                self.compile_to_string(args)
            }
            Expression::Call { name, args, .. } if name == "strings.replace_first" => {
                self.compile_replace_first(args)
            }
            Expression::Call { name, args, .. } if name == "strings.starts_with" => {
                self.compile_starts_with(args)
            }
            Expression::Call { name, args, .. } if name == "strings.ends_with" => {
                self.compile_ends_with(args)
            }
            Expression::Call { name, args, .. } if name == "strings.pad_left" => {
                self.compile_pad_left(args)
            }
            Expression::Call { name, args, .. } if name == "strings.pad_right" => {
                self.compile_pad_right(args)
            }

            Expression::Call { name, args, .. } if name == "abs" => self.compile_abs(args),
            Expression::Call { name, args, .. } if name == "sqrt" => self.compile_sqrt(args),
            Expression::Call { name, args, .. } if name == "pow" => self.compile_pow(args),
            Expression::Call { name, args, .. } if name == "floor" => self.compile_floor(args),
            Expression::Call { name, args, .. } if name == "ceil" => self.compile_ceil(args),
            Expression::Call { name, args, .. } if name == "round" => self.compile_round(args),
            Expression::Call { name, args, .. } if name == "min" => self.compile_min(args),
            Expression::Call { name, args, .. } if name == "max" => self.compile_max(args),
            Expression::Call { name, args, .. } if name == "range" => self.compile_range(args),
            Expression::Call { name, args, .. } if name == "sleep" => self.compile_sleep(args),
            Expression::Call { name, args, .. } if name == "exit" => self.compile_exit(args),
            Expression::Call { name, args, .. } if name == "env_args" => self.compile_env_args(),
            // FS functions
            Expression::Call { name, args, .. } if name == "fs_write" => {
                self.compile_fs_write(args)
            }
            Expression::Call { name, args, .. } if name == "fs_append" => {
                self.compile_fs_append(args)
            }
            Expression::Call { name, args, .. } if name == "fs_read" => self.compile_fs_read(args),
            Expression::Call { name, args, .. } if name == "fs_copy" => self.compile_fs_copy(args),
            Expression::Call { name, args, .. } if name == "fs_move" => self.compile_fs_move(args),
            Expression::Call { name, args, .. } if name == "fs_drop" => self.compile_fs_drop(args),
            Expression::Call { name, args, .. } if name == "fs_mkdir" => {
                self.compile_fs_mkdir(args)
            }
            Expression::Call { name, args, .. } if name == "fs_rmdir" => {
                self.compile_fs_rmdir(args)
            }
            Expression::Call { name, args, .. } if name == "fs_exists" => {
                self.compile_fs_exists(args)
            }
            Expression::Call { name, args, .. } if name == "fs_is_dir" => {
                self.compile_fs_is_dir(args)
            }
            Expression::Call { name, args, .. } if name == "fs_is_file" => {
                self.compile_fs_is_file(args)
            }
            Expression::Call { name, args, .. } if name == "fs_size" => self.compile_fs_size(args),
            Expression::Call { name, args, .. } if name == "fs_list" => self.compile_fs_list(args),
            Expression::Call { name, args, .. } if name == "fs_join" => self.compile_fs_join(args),
            Expression::Call { name, args, .. } if name == "fs_dir" => self.compile_fs_dir(args),
            Expression::Call { name, args, .. } if name == "fs_name" => self.compile_fs_name(args),
            Expression::Call { name, args, .. } if name == "fs_ext" => self.compile_fs_ext(args),
            // PROC functions
            Expression::Call { name, args, .. } if name == "proc_run" => {
                self.compile_proc_run(args)
            }
            Expression::Call { name, args, .. } if name == "proc_exec" => {
                self.compile_proc_exec(args)
            }
            Expression::Call { name, args, .. } if name == "proc_spawn" => {
                self.compile_proc_spawn(args)
            }
            Expression::Call { name, args, .. } if name == "proc_wait" => {
                self.compile_proc_wait(args)
            }
            Expression::Call { name, args, .. } if name == "proc_kill" => {
                self.compile_proc_kill(args)
            }
            Expression::Call { name, args, .. } if name == "proc_exit" => {
                self.compile_proc_exit(args)
            }
            Expression::Call { name, args, .. } if name == "proc_shell" => {
                self.compile_proc_shell(args)
            }
            Expression::Call { name, args, .. } if name == "proc_exists" => {
                self.compile_proc_exists(args)
            }
            // ENV functions
            Expression::Call { name, args, .. } if name == "env_get" => self.compile_env_get(args),
            Expression::Call { name, args, .. } if name == "env_set" => self.compile_env_set(args),
            Expression::Call { name, args, .. } if name == "env_cwd" => self.compile_env_cwd(),
            Expression::Call { name, args, .. } if name == "env_all" => self.compile_env_all(),
            Expression::Call { name, args, .. } if name == "time.mark" => {
                self.compile_time_mark(args)
            }
            Expression::Call { name, args, .. } if name == "time_mark" => {
                self.compile_time_mark(args)
            }
            Expression::Call { name, args, .. } if name == "time.elapsed_ms" => {
                self.compile_time_elapsed_ms(args)
            }
            Expression::Call { name, args, .. } if name == "time_elapsed_ms" => {
                self.compile_time_elapsed_ms_i64(args)
            }
            Expression::Call { name, args, .. } if name == "time_elapsed_ns" => {
                self.compile_time_elapsed_ms_i64(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.mark" => {
                self.compile_cpu_mark(args)
            }
            Expression::Call { name, args, .. } if name == "cpu_mark" => {
                self.compile_cpu_mark(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.elapsed_ms" => {
                self.compile_cpu_elapsed_ms(args)
            }
            Expression::Call { name, args, .. } if name == "cpu_elapsed_ms" => {
                self.compile_cpu_elapsed_ms_i64(args)
            }
            Expression::Call { name, args, .. } if name == "cpu_elapsed_ns" => {
                self.compile_cpu_elapsed_ms_i64(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.cycles_est" => {
                self.compile_cpu_cycles_est(args)
            }
            Expression::Call { name, args, .. } if name == "cpu_cycles_est" => {
                self.compile_cpu_cycles_est(args)
            }
            Expression::Call { name, args, .. } if name == "gpu.snapshot" => {
                self.compile_gpu_snapshot(args)
            }
            Expression::Call { name, args, .. } if name == "mem.format" => {
                self.compile_mem_format(args)
            }
            Expression::Call { name, args, .. } if name == "mem.process" => {
                self.compile_mem_process(args)
            }
            Expression::Call { name, args, .. } if name == "lists.fold" => {
                self.compile_lists_fold(args)
            }
            Expression::Call { name, args, .. } if name == "lists.map" => {
                self.compile_lists_map(args)
            }
            Expression::Call { name, args, .. } if name == "lists.filter" => {
                self.compile_lists_filter(args)
            }
            Expression::Call { name, args, .. } if name == "__type_matches" => {
                self.compile_type_matches(args)
            }
            Expression::Call {
                name,
                args,
                data_type,
                type_args,
                ..
            } => {
                // Check if this is a struct constructor call
                if data_type.is_struct_like() {
                    let normalized_name = normalize_nominal_name(name);
                    if self.user_structs.contains_key(&normalized_name) {
                        return self.compile_struct_constructor(&normalized_name, args);
                    }
                }

                let mut resolved_name = name.clone();
                let mut prepend_receiver = None;

                if let Some((receiver_name, method_name)) = name.split_once('.')
                    && let Some(struct_name) = self
                        .vars
                        .get(receiver_name)
                        .and_then(|info| info.struct_name.clone())
                {
                    let candidate_name =
                        format!("{}.{}", normalize_nominal_name(&struct_name), method_name);
                    if let Some(candidate_info) = self.user_functions.get(&candidate_name)
                        && candidate_info.params.len() == args.len() + 1
                    {
                        resolved_name = candidate_name;
                        prepend_receiver = Some(Expression::Identifier(Identifier {
                            name: receiver_name.to_string(),
                            data_type: DataType::StructNamed(struct_name.clone()),
                            line: 0,
                            column: 0,
                        }));
                    }
                }

                let fn_info = self
                    .user_functions
                    .get(&resolved_name)
                    .cloned()
                    .or_else(|| {
                        strip_root_namespace(&resolved_name)
                            .and_then(|alias| self.user_functions.get(&alias).cloned())
                    })
                    .ok_or_else(|| {
                        MireError::new(ErrorKind::Backend {
                            message: format!("Avenys unknown function '{}'", name),
                        })
                    })?;

                let mut resolved_args =
                    Vec::with_capacity(args.len() + usize::from(prepend_receiver.is_some()));
                if let Some(receiver_expr) = prepend_receiver {
                    resolved_args.push(receiver_expr);
                }
                resolved_args.extend(args.iter().cloned());

                if fn_info.params.len() != resolved_args.len() {
                    return Err(MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Avenys function '{}' expects {} args, got {}",
                            resolved_name,
                            fn_info.params.len(),
                            resolved_args.len()
                        ),
                    }));
                }
                let mut rendered_args = Vec::with_capacity(resolved_args.len());
                for (arg_expr, expected_ty) in resolved_args.iter().zip(fn_info.params.iter()) {
                    let value = self.compile_expr(arg_expr)?;
                    let casted = match expected_ty {
                        LlType::I64 => self.cast_to_i64(value)?,
                        LlType::I1 => self.cast_to_i1(value)?,
                        LlType::I8 => value,
                        LlType::F64 => value,
                        LlType::Ptr if value.ty == LlType::Ptr => value,
                        LlType::Ptr => {
                            return Err(MireError::new(ErrorKind::Runtime {
                                message: format!(
                                    "Avenys cannot cast argument for function '{}'",
                                    resolved_name
                                ),
                            }));
                        }
                        LlType::Struct(_) => value,
                    };
                    let expected_ty = expected_ty.clone();
                    rendered_args.push(format!("{} {}", self.ty(expected_ty.clone()), casted.repr));
                }
                let tmp = self.tmp();
                let ret_ty = fn_info.ret.clone();
                let call_llvm_name =
                    self.resolve_monomorph_call_symbol(&resolved_name, &fn_info, type_args);
                self.body.push(format!(
                    "  {tmp} = call {} {}({})",
                    self.ty(ret_ty.clone()),
                    call_llvm_name,
                    rendered_args.join(", ")
                ));
                Ok(LlValue {
                    ty: ret_ty,
                    repr: tmp,
                    owned: false,
                })
            }
            Expression::Pipeline { input, stage, .. } => {
                let input_val = self.compile_expr(input)?;

                match stage.as_ref() {
                    Expression::Call {
                        name,
                        args,
                        data_type: _,
                        ..
                    } => {
                        if name == "len" {
                            return self.compile_pipeline_len(input, input_val);
                        }

                        let mut all_args = vec![input_val];
                        for arg in args {
                            let arg_val = self.compile_expr(arg)?;
                            all_args.push(arg_val);
                        }

                        let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
                            MireError::new(ErrorKind::Backend {
                                message: format!("Avenys unknown function '{}'", name),
                            })
                        })?;

                        let mut rendered_args = Vec::with_capacity(all_args.len());
                        for (arg_val, expected_ty) in all_args.iter().zip(fn_info.params.iter()) {
                            let casted = match expected_ty {
                                LlType::I64 => self.cast_to_i64(arg_val.clone())?,
                                LlType::I1 => self.cast_to_i1(arg_val.clone())?,
                                _ => arg_val.clone(),
                            };
                            rendered_args.push(format!(
                                "{} {}",
                                self.ty(expected_ty.clone()),
                                casted.repr
                            ));
                        }

                        let tmp = self.tmp();
                        let ret_ty = fn_info.ret.clone();
                        self.body.push(format!(
                            "  {tmp} = call {} {}({})",
                            self.ty(ret_ty.clone()),
                            fn_info.llvm_name,
                            rendered_args.join(", ")
                        ));
                        Ok(LlValue {
                            ty: ret_ty,
                            repr: tmp,
                            owned: false,
                        })
                    }
                    Expression::Identifier(Identifier { name, .. }) => {
                        if name == "len" {
                            return self.compile_pipeline_len(input, input_val);
                        }

                        let all_args = [input_val];

                        let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
                            MireError::new(ErrorKind::Backend {
                                message: format!("Avenys unknown function '{}'", name),
                            })
                        })?;

                        let mut rendered_args = Vec::with_capacity(all_args.len());
                        for (arg_val, expected_ty) in all_args.iter().zip(fn_info.params.iter()) {
                            let casted = match expected_ty {
                                LlType::I64 => self.cast_to_i64(arg_val.clone())?,
                                LlType::I1 => self.cast_to_i1(arg_val.clone())?,
                                _ => arg_val.clone(),
                            };
                            rendered_args.push(format!(
                                "{} {}",
                                self.ty(expected_ty.clone()),
                                casted.repr
                            ));
                        }

                        let tmp = self.tmp();
                        let ret_ty = fn_info.ret.clone();
                        self.body.push(format!(
                            "  {tmp} = call {} {}({})",
                            self.ty(ret_ty.clone()),
                            fn_info.llvm_name,
                            rendered_args.join(", ")
                        ));
                        Ok(LlValue {
                            ty: ret_ty,
                            repr: tmp,
                            owned: false,
                        })
                    }
                    Expression::Closure {
                        params,
                        body,
                        return_type,
                        capture: _,
                    } => self.compile_pipeline_closure(input_val, params, body, return_type),
                    _ => Err(MireError::new(ErrorKind::Runtime {
                        message: "Pipeline stage must be a function call, identifier, or closure"
                            .to_string(),
                    })),
                }
            }
            Expression::NamedArg { value, .. } => self.compile_expr(value),
            Expression::Tuple { elements, .. } => {
                self.compile_list_literal(elements, &DataType::Unknown)
            }
            Expression::Box { value, .. } => self.compile_expr(value),
            Expression::Closure { .. } => Ok(self.string_value("<closure>")),
        };
        result.map_err(|err| self.attach_context(err))
    }

    pub(super) fn attach_context(&self, err: MireError) -> MireError {
        if err.line == 1 && err.column == 1 {
            err.with_position(self.current_line.max(1), self.current_column.max(1))
        } else {
            err
        }
    }

    fn function_name_from_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(ident) => {
                if self.user_functions.contains_key(&ident.name) {
                    Some(ident.name.clone())
                } else if let Some(alias) = strip_root_namespace(&ident.name)
                    && self.user_functions.contains_key(&alias)
                {
                    Some(alias)
                } else {
                    self.function_aliases.get(&ident.name).cloned()
                }
            }
            _ => None,
        }
    }

    fn function_signature_from_expr(&self, expr: &Expression) -> Option<FnInfo> {
        match expr {
            Expression::Identifier(ident) => self
                .user_functions
                .get(&ident.name)
                .cloned()
                .or_else(|| {
                    strip_root_namespace(&ident.name)
                        .and_then(|alias| self.user_functions.get(&alias).cloned())
                })
                .or_else(|| self.function_value_signatures.get(&ident.name).cloned()),
            _ => None,
        }
    }

    pub(super) fn statement_location(statement: &Statement) -> (usize, usize) {
        match statement {
            Statement::Let {
                name_line,
                name_column,
                ..
            } => (*name_line, *name_column),
            Statement::Assignment { value, .. }
            | Statement::Expression(value)
            | Statement::Drop { value }
            | Statement::New {
                value: Some(value), ..
            }
            | Statement::Own {
                value: Some(value), ..
            }
            | Statement::Move { value, .. } => Self::expression_location(value),
            Statement::Return(Some(value)) => Self::expression_location(value),
            Statement::If { condition, .. } | Statement::While { condition, .. } => {
                Self::expression_location(condition)
            }
            Statement::For { iterable, .. } | Statement::Find { iterable, .. } => {
                Self::expression_location(iterable)
            }
            Statement::Match { value, .. } => Self::expression_location(value),
            _ => (1, 1),
        }
    }

    pub(super) fn expression_location(expression: &Expression) -> (usize, usize) {
        match expression {
            Expression::Identifier(ident) => (ident.line.max(1), ident.column.max(1)),
            Expression::BinaryOp { left, .. }
            | Expression::NamedArg { value: left, .. }
            | Expression::Reference { expr: left, .. }
            | Expression::Dereference { expr: left, .. }
            | Expression::Box { value: left, .. }
            | Expression::Pipeline { input: left, .. }
            | Expression::Try { expr: left, .. }
            | Expression::Ok { value: left, .. }
            | Expression::Err { value: left, .. } => Self::expression_location(left),
            Expression::UnaryOp { operand, .. } => Self::expression_location(operand),
            Expression::Call { args, .. }
            | Expression::List { elements: args, .. }
            | Expression::Tuple { elements: args, .. } => args
                .first()
                .map(Self::expression_location)
                .unwrap_or((1, 1)),
            Expression::Dict { entries, .. } => entries
                .first()
                .map(|(key, _)| Self::expression_location(key))
                .unwrap_or((1, 1)),
            Expression::Index { target, .. } | Expression::MemberAccess { target, .. } => {
                Self::expression_location(target)
            }
            Expression::Closure { body, .. } => {
                body.first().map(Self::statement_location).unwrap_or((1, 1))
            }
            Expression::Match { value, .. } => Self::expression_location(value),
            Expression::EnumVariant { payloads, .. } => payloads
                .first()
                .map(Self::expression_location)
                .unwrap_or((1, 1)),
            Expression::Literal(_) | Expression::EnumVariantPath { .. } => (1, 1),
        }
    }

    pub(super) fn compile_input_expr(
        &mut self,
        args: &[Expression],
        data_type: &DataType,
    ) -> Result<LlValue> {
        // ireru accepts 0 or 1 argument:
        //   ireru()          — read line with no prompt
        //   ireru("prompt")  — show prompt, then read line
        // Optional type annotation: ireru() :i64, ireru("> ") :f64, ireru() :bool
        if args.len() > 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys ireru expects 0 or 1 argument".to_string(),
            }));
        }

        let prompt = if args.is_empty() {
            self.null_value()
        } else {
            self.compile_expr(&args[0])?
        };

        let input = self.tmp();
        self.body.push(format!(
            "  {input} = call ptr @rt_read_line(ptr {})",
            prompt.repr
        ));

        match data_type {
            DataType::I64 | DataType::I32 | DataType::I16 | DataType::I8 => {
                let parsed = self.tmp();
                self.body
                    .push(format!("  {parsed} = call i64 @atoll(ptr {input})"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: parsed,
                    owned: false,
                })
            }
            DataType::F64 => {
                let parsed = self.tmp();
                self.body
                    .push(format!("  {parsed} = call double @atof(ptr {input})"));
                Ok(LlValue {
                    ty: LlType::F64,
                    repr: parsed,
                    owned: false,
                })
            }
            DataType::Bool => {
                let true_str = self.string_value("true");
                let one_str = self.string_value("1");
                let cmp_true = self.tmp();
                let is_true = self.tmp();
                let cmp_one = self.tmp();
                let is_one = self.tmp();
                let result = self.tmp();
                self.body.push(format!(
                    "  {cmp_true} = call i32 @strcmp(ptr {input}, ptr {})",
                    true_str.repr
                ));
                self.body
                    .push(format!("  {is_true} = icmp eq i32 {cmp_true}, 0"));
                self.body.push(format!(
                    "  {cmp_one} = call i32 @strcmp(ptr {input}, ptr {})",
                    one_str.repr
                ));
                self.body
                    .push(format!("  {is_one} = icmp eq i32 {cmp_one}, 0"));
                self.body
                    .push(format!("  {result} = or i1 {is_true}, {is_one}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            _ => Ok(LlValue {
                ty: LlType::Ptr,
                repr: input,
                owned: true,
            }),
        }
    }

    pub(super) fn runtime_kind_code(&self, data_type: &DataType) -> i64 {
        match data_type {
            DataType::Bool => 2,
            DataType::Str => 3,
            DataType::Dict | DataType::Map { .. } => 4,
            DataType::List
            | DataType::Vector { .. }
            | DataType::Set
            | DataType::Tuple
            | DataType::Array { .. }
            | DataType::Slice { .. } => 5,
            DataType::Struct
            | DataType::StructNamed(_)
            | DataType::Enum
            | DataType::EnumNamed(_) => 6,
            DataType::Function | DataType::DynTrait { .. } => 7,
            DataType::Result { .. } => 8,
            _ => 1,
        }
    }

    pub(super) fn resolve_monomorph_call_symbol(
        &mut self,
        resolved_name: &str,
        fn_info: &FnInfo,
        type_args: &[DataType],
    ) -> String {
        if type_args.is_empty() {
            return fn_info.llvm_name.clone();
        }

        let suffix = type_args
            .iter()
            .map(|ty| sanitize_symbol(&format!("{ty:?}").to_lowercase()))
            .collect::<Vec<_>>()
            .join("_");
        let wrapper_name = format!(
            "@fn_{}__mono_{}",
            sanitize_symbol(resolved_name),
            sanitize_symbol(&suffix)
        );

        if self.emitted_monomorph_wrappers.insert(wrapper_name.clone()) {
            let params_sig = fn_info
                .params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg_{idx}", self.ty(ty.clone())))
                .collect::<Vec<_>>()
                .join(", ");
            let args_sig = fn_info
                .params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg_{idx}", self.ty(ty.clone())))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = self.ty(fn_info.ret.clone());
            let body = if fn_info.ret == LlType::I64 && !fn_info.returns_value {
                format!(
                    "define {ret} {wrapper_name}({params_sig}) {{\n  call {ret} {}({args_sig})\n  ret i64 0\n}}\n",
                    fn_info.llvm_name
                )
            } else {
                format!(
                    "define {ret} {wrapper_name}({params_sig}) {{\n  %mono_ret = call {ret} {}({args_sig})\n  ret {ret} %mono_ret\n}}\n",
                    fn_info.llvm_name
                )
            };
            self.functions.push(body);
        }

        wrapper_name
    }

    fn llvm_fn_name(&mut self, name: &str) -> String {
        if name == "main" {
            "@mire_main".to_string()
        } else {
            let counter = {
                let c = self.next_fn_id.entry(name.to_string()).or_insert(0usize);
                *c += 1;
                *c
            };
            format!("@fn_{}_{}", sanitize_symbol(name), counter)
        }
    }

    pub(super) fn compile_function_ir(
        &mut self,
        name: &str,
        params: &[(String, DataType)],
        body: &[Statement],
        ret: LlType,
        returns_value: bool,
    ) -> Result<String> {
        let saved_allocas = std::mem::take(&mut self.entry_allocas);
        let saved_body = std::mem::take(&mut self.body);
        let saved_vars = std::mem::take(&mut self.vars);
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_return = self.current_return.clone();
        let saved_function = self.current_function.clone();
        self.current_function = name.to_string();
        self.current_return = ret.clone();
        let method_owner = name.split_once('.').map(|(owner, _)| owner.to_string());

        for (param_name, param_data_type) in params.iter() {
            let param_ty = if param_name == "self" {
                LlType::Ptr
            } else {
                self.map_type(param_data_type)?
            };
            let ptr = self.tmp();
            let arg_name = format!("%arg_{}", sanitize_symbol(param_name));
            self.entry_allocas
                .push(format!("  {ptr} = alloca {}", self.ty(param_ty.clone())));
            self.body.push(format!(
                "  store {} {}, ptr {}",
                self.ty(param_ty.clone()),
                arg_name,
                ptr
            ));

            let param_struct_name = match param_data_type {
                DataType::StructNamed(name) => Some(name.clone()),
                _ => {
                    let ty_str = self.ty(param_ty.clone());
                    self.user_structs
                        .iter()
                        .find(|(_, info)| self.render_struct_ty(&info.fields) == ty_str)
                        .map(|(name, _)| name.clone())
                }
            };

            let final_data_type = if param_name == "self" {
                method_owner
                    .clone()
                    .map(DataType::StructNamed)
                    .unwrap_or(DataType::Struct)
            } else {
                param_data_type.clone()
            };

            let final_struct_name = if param_name == "self" {
                method_owner.clone()
            } else {
                param_struct_name
            };

            self.vars.insert(
                param_name.clone(),
                VarInfo {
                    ptr,
                    ty: param_ty.clone(),
                    data_type: final_data_type,
                    owns_heap_string: false,
                    struct_name: final_struct_name,
                },
            );
        }

        for stmt in body {
            self.compile_statement(stmt)?;
        }

        let ret_clone = ret.clone();
        self.emit_string_locals_cleanup();
        if body
            .iter()
            .all(|stmt| !matches!(stmt, Statement::Return(_)))
        {
            if returns_value {
                if let Some(Statement::Expression(expr)) = body.last() {
                    let value = self.compile_expr(expr)?;
                    let ret = self.cast_to_type(value, ret_clone.clone())?;
                    let result_ptr = self.tmp();
                    self.body.push(format!(
                        "  {result_ptr} = alloca {}",
                        self.ty(ret_clone.clone())
                    ));
                    self.body.push(format!(
                        "  store {} {}, ptr {}",
                        self.ty(ret_clone.clone()),
                        ret.repr,
                        result_ptr
                    ));
                    self.body.push(format!(
                        "  %ret_val = load {}, ptr {}",
                        self.ty(ret_clone.clone()),
                        result_ptr
                    ));
                    self.body
                        .push(format!("  ret {} %ret_val", self.ty(ret_clone.clone())));
                } else {
                    let default = self.default_value(ret_clone.clone());
                    self.body.push(format!(
                        "  ret {} {}",
                        self.ty(ret_clone.clone()),
                        default.repr
                    ));
                }
            } else {
                let default = self.default_value(ret_clone.clone());
                self.body.push(format!(
                    "  ret {} {}",
                    self.ty(ret_clone.clone()),
                    default.repr
                ));
            }
        }

        let args = params
            .iter()
            .map(|(name, data_type)| {
                let ty = if name == "self" {
                    LlType::Ptr
                } else {
                    self.map_type(data_type).unwrap_or(LlType::I64)
                };
                format!("{} %arg_{}", self.ty(ty), sanitize_symbol(name))
            })
            .collect::<Vec<_>>()
            .join(", ");

        let llvm_name = self.llvm_fn_name(name);
        let mut lines = Vec::new();
        lines.push(format!(
            "define {} {}({}) {{",
            self.ty(ret_clone.clone()),
            llvm_name,
            args
        ));
        lines.push("entry:".to_string());
        lines.extend(self.entry_allocas.clone());
        lines.extend(self.body.clone());
        lines.push("}".to_string());

        self.entry_allocas = saved_allocas;
        self.body = saved_body;
        self.vars = saved_vars;
        self.loop_stack = saved_loop_stack;
        self.current_return = saved_return;
        self.current_function = saved_function;

        Ok(lines.join("\n"))
    }
}
