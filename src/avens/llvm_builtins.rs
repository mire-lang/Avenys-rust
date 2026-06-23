use super::*;

impl LlvmIrGen {
    pub(super) fn compile_float(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys float(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_f64(value)
    }

    pub(super) fn compile_int(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys int(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_i64(value)
    }

    pub(super) fn compile_bool(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys bool(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_i1(value)
    }

    pub(super) fn compile_abs(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys abs(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body
            .push(format!("  {tmp} = call i64 @abs(i64 {})", value.repr));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_sqrt(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys sqrt(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let input = self.cast_to_f64(value)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call double @rt_math_sqrt(double {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::F64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_pow(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys pow(...) expects 2 arguments".to_string(),
            }));
        }
        let base = self.compile_expr(&args[0])?;
        let exp = self.compile_expr(&args[1])?;
        let base = self.cast_to_f64(base)?;
        let exp = self.cast_to_f64(exp)?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call double @rt_math_pow(double {}, double {})",
            base.repr, exp.repr
        ));
        Ok(LlValue {
            ty: LlType::F64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_floor(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys floor(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let input = self.cast_to_f64(value)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_math_floor(double {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_ceil(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys ceil(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let input = self.cast_to_f64(value)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_math_ceil(double {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_round(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys round(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let input = self.cast_to_f64(value)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @rt_math_round(double {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_min(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys min(...) expects 2 arguments".to_string(),
            }));
        }
        let lhs = self.compile_expr(&args[0])?;
        let rhs = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @llvm.smin.i64(i64 {}, i64 {})",
            lhs.repr, rhs.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_max(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys max(...) expects 2 arguments".to_string(),
            }));
        }
        let lhs = self.compile_expr(&args[0])?;
        let rhs = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @llvm.smax.i64(i64 {}, i64 {})",
            lhs.repr, rhs.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_range(&mut self, _args: &[Expression]) -> Result<LlValue> {
        Ok(self.string_value("<range>"))
    }

    pub(super) fn compile_sleep(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys sleep(...) expects 1 argument".to_string(),
            }));
        }
        let ms = self.compile_expr(&args[0])?;
        self.body
            .push(format!("  call void @usleep(i64 {})", ms.repr));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_exit(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys exit(...) expects 1 argument".to_string(),
            }));
        }
        let code = self.compile_expr(&args[0])?;
        self.body.push(format!("  ret i32 {}", code.repr));
        Ok(LlValue {
            ty: LlType::I64,
            repr: code.repr,
            owned: false,
        })
    }

    pub(super) fn compile_env_args(&mut self) -> Result<LlValue> {
        let tmp = self.tmp();
        let argc_val = self.tmp();
        let argv_val = self.tmp();
        self.body
            .push(format!("  {argc_val} = load i32, ptr @.argc"));
        self.body
            .push(format!("  {argv_val} = load ptr, ptr @.argv"));
        self.body.push(format!(
            "  {tmp} = call ptr @rt_get_args(i32 {argc_val}, ptr {argv_val})"
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_write(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_write expects 2 arguments".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let content = self.compile_expr(&args[1])?;
        self.body.push(format!(
            "  call i32 @pal_fs_write(ptr {}, ptr {})",
            path.repr, content.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_fs_append(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_append expects 2 arguments".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let content = self.compile_expr(&args[1])?;
        self.body.push(format!(
            "  call i32 @pal_fs_append(ptr {}, ptr {})",
            path.repr, content.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_fs_read(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_read expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_fs_read(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_fs_copy(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_copy expects 2 arguments".to_string(),
            }));
        }
        let src = self.compile_expr(&args[0])?;
        let dst = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i32 @pal_fs_copy(ptr {}, ptr {})",
            src.repr, dst.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_move(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_move expects 2 arguments".to_string(),
            }));
        }
        let src = self.compile_expr(&args[0])?;
        let dst = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i32 @pal_fs_move(ptr {}, ptr {})",
            src.repr, dst.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_drop(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_drop expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i32 @pal_fs_delete(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_mkdir(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_mkdir expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i32 @pal_fs_mkdir(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_rmdir(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_rmdir expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i32 @pal_fs_rmdir(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_exists(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_exists expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_fs_exists(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_is_dir(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_is_dir expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_fs_is_dir(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_is_file(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_is_file expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_fs_is_file(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_size(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_size expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_fs_size(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_list(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_list expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_fs_list(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_fs_join(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_join expects 2 arguments".to_string(),
            }));
        }
        let a = self.compile_expr(&args[0])?;
        let b = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_fs_join(ptr {}, ptr {})",
            a.repr, b.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_fs_dir(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_dir expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body
            .push(format!("  {tmp} = call ptr @pal_fs_dir(ptr {})", path.repr));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_fs_name(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_name expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_fs_name(ptr {})",
            path.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_fs_ext(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "fs_ext expects 1 argument".to_string(),
            }));
        }
        let path = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body
            .push(format!("  {tmp} = call ptr @pal_fs_ext(ptr {})", path.repr));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    // ==================== PROC Functions ====================

    pub(super) fn compile_proc_run(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_run expects 1 argument".to_string(),
            }));
        }
        let cmd = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_proc_run(ptr {})",
            cmd.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_proc_exec(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_exec expects 1 argument".to_string(),
            }));
        }
        let cmd = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_proc_exec(ptr {})",
            cmd.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_proc_spawn(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_spawn expects 1 argument".to_string(),
            }));
        }
        let cmd = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_proc_spawn(ptr {})",
            cmd.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_proc_wait(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_wait expects 1 argument".to_string(),
            }));
        }
        let pid = self.compile_expr(&args[0])?;
        let pid = self.cast_to_i64(pid)?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_proc_wait(i64 {})",
            pid.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_proc_kill(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_kill expects 1 argument".to_string(),
            }));
        }
        let pid = self.compile_expr(&args[0])?;
        let pid = self.cast_to_i64(pid)?;
        self.body
            .push(format!("  call i32 @pal_proc_kill(i64 {})", pid.repr));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_proc_exit(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_exit expects 1 argument".to_string(),
            }));
        }
        let code = self.compile_expr(&args[0])?;
        let code = self.cast_to_i64(code)?;
        self.body
            .push(format!("  call void @pal_proc_exit(i64 {})", code.repr));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_proc_shell(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_shell expects 1 argument".to_string(),
            }));
        }
        let cmd = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_proc_shell(ptr {})",
            cmd.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_proc_exists(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "proc_exists expects 1 argument".to_string(),
            }));
        }
        let pid = self.compile_expr(&args[0])?;
        let pid = self.cast_to_i64(pid)?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pal_proc_exists(i64 {})",
            pid.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    // ==================== ENV Functions ====================

    pub(super) fn compile_env_get(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "env_get expects 1 argument".to_string(),
            }));
        }
        let name = self.compile_expr(&args[0])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call ptr @pal_env_get(ptr {})",
            name.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_env_set(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "env_set expects 2 arguments".to_string(),
            }));
        }
        let name = self.compile_expr(&args[0])?;
        let value = self.compile_expr(&args[1])?;
        self.body.push(format!(
            "  call i32 @pal_env_set(ptr {}, ptr {})",
            name.repr, value.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    pub(super) fn compile_env_cwd(&mut self) -> Result<LlValue> {
        let tmp = self.tmp();
        self.body.push(format!("  {tmp} = call ptr @pal_env_cwd()"));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: true,
        })
    }

    pub(super) fn compile_env_all(&mut self) -> Result<LlValue> {
        let tmp = self.tmp();
        self.body.push(format!("  {tmp} = call ptr @pal_env_all()"));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_time_mark(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let tmp = self.tmp();
        self.body
            .push(format!("  {tmp} = call i64 @pal_time_mark()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    pub(super) fn compile_time_elapsed_ms(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys time.elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call ptr @rt_time_elapsed_ms_str(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: diff,
            owned: true,
        })
    }

    pub(super) fn compile_time_elapsed_ms_i64(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys time_elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let mark = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call i64 @pal_time_elapsed_ms(i64 {})",
            mark.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: diff,
            owned: false,
        })
    }

    pub(super) fn compile_cpu_mark(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call i64 @pal_cpu_mark()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_cpu_elapsed_ms(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cpu.elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call ptr @rt_cpu_elapsed_ms_str(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: diff,
            owned: true,
        })
    }

    pub(super) fn compile_cpu_elapsed_ms_i64(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cpu_elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let mark = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call i64 @pal_cpu_elapsed_ms(i64 {})",
            mark.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: diff,
            owned: false,
        })
    }

    pub(super) fn compile_cpu_cycles_est(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cpu.cycles_est expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call i64 @pal_cpu_cycles_est(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: diff,
            owned: false,
        })
    }

    pub(super) fn compile_gpu_snapshot(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call ptr @pal_gpu_snapshot()"));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_mem_format(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys mem.format expects 1 argument".to_string(),
            }));
        }
        let value_expr = self.compile_expr(&args[0])?;
        let value = self.cast_to_i64(value_expr)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @pal_mem_format(i64 {})",
            value.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    pub(super) fn compile_mem_process(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call i64 @pal_mem_process_bytes()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    pub(super) fn compile_len(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys len(...) expects exactly 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        let data_type = match &args[0] {
            Expression::Identifier(identifier) => &identifier.data_type,
            Expression::BinaryOp { data_type, .. }
            | Expression::UnaryOp { data_type, .. }
            | Expression::NamedArg { data_type, .. }
            | Expression::Call { data_type, .. }
            | Expression::List { data_type, .. }
            | Expression::Dict { data_type, .. }
            | Expression::Tuple { data_type, .. }
            | Expression::Index { data_type, .. }
            | Expression::MemberAccess { data_type, .. }
            | Expression::Reference { data_type, .. }
            | Expression::Dereference { data_type, .. }
            | Expression::Box { data_type, .. }
            | Expression::Pipeline { data_type, .. }
            | Expression::Match { data_type, .. }
            | Expression::Try { data_type, .. }
            | Expression::Ok { data_type, .. }
            | Expression::Err { data_type, .. }
            | Expression::EnumVariantPath { data_type, .. }
            | Expression::EnumVariant { data_type, .. } => data_type,
            Expression::Literal(Literal::Str(_)) => &DataType::Str,
            Expression::Literal(Literal::List(_)) => &DataType::List,
            Expression::Literal(_) => &DataType::Unknown,
            Expression::Closure { return_type, .. } => return_type,
        };

        match data_type {
            DataType::Str => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = call i64 @strlen(ptr {})", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            DataType::List | DataType::Vector { .. } => self.compile_list_len(args),
            _ => match value.ty {
                LlType::Ptr => self.compile_list_len(args),
                LlType::I64 | LlType::I1 | LlType::F64 | LlType::I8 => Ok(LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                }),
                LlType::Struct(_) => Err(MireError::new(ErrorKind::Backend {
                    message: "Struct type not supported here".to_string(),
                })),
            },
        }
    }

    pub(super) fn closure_statements<'a>(
        &self,
        expr: &'a Expression,
        ctx: &str,
    ) -> Result<&'a [Statement]> {
        match expr {
            Expression::Closure { params, body, .. } if params.is_empty() => Ok(body),
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys expects a zero-arg closure for {}", ctx),
            })),
        }
    }

    pub(super) fn closure_return_expr<'a>(
        &self,
        expr: &'a Expression,
        ctx: &str,
    ) -> Result<&'a Expression> {
        match expr {
            Expression::Closure { params, body, .. } if params.is_empty() => {
                if let [Statement::Return(Some(value))] = body.as_slice() {
                    Ok(value)
                } else {
                    Err(MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Avenys expects {} closure to be a single return expression",
                            ctx
                        ),
                    }))
                }
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys expects a zero-arg closure for {}", ctx),
            })),
        }
    }
}
