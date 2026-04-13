use crate::compiler::analyze_program;
use crate::error::{ErrorKind, MireError, Result};
use crate::loader::load_program_from_file;
use crate::parser::ast::{DataType, Expression, Identifier, Literal, Program, Statement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Debug,
    Release,
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub mode: BuildMode,
    pub debug_dump: bool,
    pub output: Option<PathBuf>,
    pub emit_binary: bool,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub binary_path: PathBuf,
    pub ir_path: PathBuf,
    pub optimized_ir_path: PathBuf,
    pub used_optimizations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireManifest {
    #[serde(alias = "package")]
    pub project: MireProject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireProject {
    pub name: String,
    pub version: String,
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLock {
    #[serde(alias = "package")]
    pub project: MireLockProject,
    pub build: MireLockBuild,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLockProject {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLockBuild {
    pub llvm_version: String,
    pub profile: String,
}

pub fn load_project_manifest(cwd: &Path) -> Result<Option<MireManifest>> {
    let manifest_path = project_manifest_path(cwd);
    if !manifest_path.exists() {
        let legacy = cwd.join("Mire.toml");
        if !legacy.exists() {
            return Ok(None);
        }
        return load_manifest_file(&legacy);
    }

    load_manifest_file(&manifest_path)
}

fn load_manifest_file(manifest_path: &Path) -> Result<Option<MireManifest>> {
    if !manifest_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(manifest_path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not read '{}': {}", manifest_path.display(), err),
        })
    })?;

    let manifest: MireManifest = toml::from_str(&raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Invalid Mire.toml: {}", err),
        })
    })?;

    Ok(Some(manifest))
}

pub fn write_lock_file(cwd: &Path, manifest: &MireManifest, mode: BuildMode) -> Result<()> {
    let llvm_version = llvm_version()?;
    let lock = MireLock {
        project: MireLockProject {
            name: manifest.project.name.clone(),
            version: manifest.project.version.clone(),
        },
        build: MireLockBuild {
            llvm_version,
            profile: match mode {
                BuildMode::Debug => "debug".to_string(),
                BuildMode::Release => "release".to_string(),
            },
        },
    };

    let raw = toml::to_string_pretty(&lock).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not serialize Mire.lock: {}", err),
        })
    })?;

    fs::write(project_lock_path(cwd), raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not write project.lock: {}", err),
        })
    })?;

    Ok(())
}

pub fn compile_file_with_avenys(source_path: &Path, options: &BuildOptions) -> Result<BuildResult> {
    let source = fs::read_to_string(source_path)?;
    let mut program = load_program_from_file(source_path)?;
    analyze_program(&mut program, &source)?;

    let output_dir = default_output_dir(source_path, options.mode);
    fs::create_dir_all(&output_dir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!(
                "Could not create build directory '{}': {}",
                output_dir.display(),
                err
            ),
        })
    })?;

    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    let binary_path = options
        .output
        .clone()
        .unwrap_or_else(|| output_dir.join(stem));
    let ir_path = output_dir.join(format!("{stem}.ll"));
    let optimized_ir_path = output_dir.join(format!("{stem}.opt.ll"));

    let ir = LlvmIrGen::new().compile_program(&program)?;
    fs::write(&ir_path, ir).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not write '{}': {}", ir_path.display(), err),
        })
    })?;

    let final_ir = if matches!(options.mode, BuildMode::Release) {
        run_command(
            Command::new("opt")
                .arg("-S")
                .arg("-O3")
                .arg(&ir_path)
                .arg("-o")
                .arg(&optimized_ir_path),
            "opt",
        )?;
        optimized_ir_path.clone()
    } else {
        fs::copy(&ir_path, &optimized_ir_path).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Could not prepare debug IR '{}': {}",
                    optimized_ir_path.display(),
                    err
                ),
            })
        })?;
        optimized_ir_path.clone()
    };

    if options.emit_binary {
        let mut clang = Command::new("clang");
        let runtime_support =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/avens/runtime_support.c");
        clang
            .arg(&final_ir)
            .arg(&runtime_support)
            .arg("-o")
            .arg(&binary_path);
        if matches!(options.mode, BuildMode::Release) {
            clang.arg("-O3");
        } else {
            clang.arg("-O0");
        }
        run_command(&mut clang, "clang")?;
    }

    Ok(BuildResult {
        binary_path,
        ir_path,
        optimized_ir_path,
        used_optimizations: matches!(options.mode, BuildMode::Release),
    })
}

pub fn default_output_dir(source_path: &Path, mode: BuildMode) -> PathBuf {
    if let Some(project_root) =
        find_project_root(source_path.parent().unwrap_or_else(|| Path::new(".")))
    {
        return project_root.join("bin").join(match mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        });
    }

    source_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("build")
        .join(match mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        })
}

pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if project_manifest_path(path).exists() || path.join("Mire.toml").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

pub fn project_manifest_path(cwd: &Path) -> PathBuf {
    cwd.join("project.toml")
}

pub fn project_lock_path(cwd: &Path) -> PathBuf {
    cwd.join("project.lock")
}

fn run_command(command: &mut Command, label: &str) -> Result<()> {
    let output = command.output().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Failed to run {}: {}", label, err),
        })
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(MireError::new(ErrorKind::Runtime {
        message: format!(
            "{} failed with status {}.\nstdout:\n{}\nstderr:\n{}",
            label,
            output.status,
            stdout.trim(),
            stderr.trim()
        ),
    }))
}

fn llvm_version() -> Result<String> {
    let output = Command::new("llvm-config")
        .arg("--version")
        .output()
        .map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Failed to run llvm-config: {}", err),
            })
        })?;
    if !output.status.success() {
        return Err(MireError::new(ErrorKind::Runtime {
            message: "llvm-config --version failed".to_string(),
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LlType {
    I64,
    I1,
    Ptr,
    Struct(Vec<LlType>),
}

#[derive(Debug, Clone)]
struct LlValue {
    ty: LlType,
    repr: String,
    owned: bool,
}

#[derive(Debug, Clone)]
struct VarInfo {
    ptr: String,
    ty: LlType,
    data_type: DataType,
    owns_heap_string: bool,
    struct_name: Option<String>,
}

#[derive(Debug, Clone)]
struct FnInfo {
    llvm_name: String,
    params: Vec<LlType>,
    ret: LlType,
}

#[derive(Debug, Clone)]
struct LoopLabels {
    break_label: String,
    continue_label: String,
}

#[derive(Debug, Clone)]
struct StructInfo {
    fields: Vec<LlType>,
    field_indices: HashMap<String, usize>,
}

struct LlvmIrGen {
    strings: Vec<String>,
    functions: Vec<String>,
    entry_allocas: Vec<String>,
    body: Vec<String>,
    vars: HashMap<String, VarInfo>,
    user_functions: HashMap<String, FnInfo>,
    user_structs: HashMap<String, StructInfo>,
    loop_stack: Vec<LoopLabels>,
    current_return: LlType,
    next_tmp: usize,
    next_label: usize,
    debug_enabled: bool,
}

const AVENYS_DEBUG: bool = false;

impl LlvmIrGen {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            functions: Vec::new(),
            entry_allocas: Vec::new(),
            body: Vec::new(),
            vars: HashMap::new(),
            user_functions: HashMap::new(),
            user_structs: HashMap::new(),
            loop_stack: Vec::new(),
            current_return: LlType::I64,
            next_tmp: 0,
            next_label: 0,
            debug_enabled: false,
        }
    }

    #[inline]
    fn debug(&self, msg: &str) {
        eprintln!("[AVENYS] {}", msg);
    }

    #[inline]
    fn debug_value(&self, name: &str, value: &LlValue) {
        eprintln!("[AVENYS] {} = {} (ty: {:?})", name, value.repr, value.ty);
    }

    fn compile_program(mut self, program: &Program) -> Result<String> {
        // First pass: collect struct definitions
        for stmt in &program.statements {
            if let Statement::Type { name, fields, .. } = stmt {
                let mut field_types = Vec::new();
                let mut field_indices = HashMap::new();

                for (idx, field_stmt) in fields.iter().enumerate() {
                    if let Statement::Let {
                        name: field_name,
                        data_type,
                        ..
                    } = field_stmt
                    {
                        field_types.push(self.map_type(data_type)?);
                        field_indices.insert(field_name.clone(), idx);
                    }
                }

                self.user_structs.insert(
                    name.clone(),
                    StructInfo {
                        fields: field_types,
                        field_indices,
                    },
                );
            }
        }

        // Second pass: collect function signatures
        for stmt in &program.statements {
            if let Statement::Function {
                name,
                params,
                return_type,
                ..
            } = stmt
            {
                let llvm_name = if name == "main" {
                    "@mire_main".to_string()
                } else {
                    format!("@fn_{}", sanitize_symbol(name))
                };
                let param_types = params
                    .iter()
                    .map(|(_, ty)| self.map_type(ty))
                    .collect::<Result<Vec<_>>>()?;
                let ret = if name == "main" {
                    LlType::I64
                } else {
                    self.map_type(return_type)?
                };
                self.user_functions.insert(
                    name.clone(),
                    FnInfo {
                        llvm_name,
                        params: param_types,
                        ret,
                    },
                );
            }
        }

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
                let fn_ir = self.compile_function_ir(name, params, body, ret)?;
                self.functions.push(fn_ir);
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

        let mut out = Vec::new();
        out.push("declare i32 @printf(ptr, ...)".to_string());
        out.push("declare i32 @scanf(ptr, ...)".to_string());
        out.push("declare i64 @strlen(ptr)".to_string());
        out.push("declare i64 @clock()".to_string());
        out.push("declare ptr @malloc(i64)".to_string());
        out.push("declare void @free(ptr)".to_string());
        out.push("declare ptr @realloc(ptr, i64)".to_string());
        out.push("declare ptr @memcpy(ptr, ptr, i64)".to_string());
        out.push("declare i32 @memcmp(ptr, ptr, i64)".to_string());
        out.push("declare i32 @strcmp(ptr, ptr)".to_string());
        out.push("declare i32 @getpagesize()".to_string());
        out.push("declare i64 @getpid()".to_string());
        out.push("declare i64 @mire_wall_mark_ns()".to_string());
        out.push("declare i64 @mire_wall_elapsed_ms(i64)".to_string());
        out.push("declare ptr @mire_wall_elapsed_ms_str(i64)".to_string());
        out.push("declare i64 @mire_cpu_mark_ns()".to_string());
        out.push("declare i64 @mire_cpu_elapsed_ms(i64)".to_string());
        out.push("declare ptr @mire_cpu_elapsed_ms_str(i64)".to_string());
        out.push("declare i64 @mire_cpu_cycles_est(i64)".to_string());
        out.push("declare i64 @mire_mem_process_bytes()".to_string());
        out.push("declare ptr @mire_mem_format(i64)".to_string());
        out.push("declare ptr @mire_gpu_snapshot()".to_string());
        out.push("declare ptr @mire_i64_to_string(i64)".to_string());
        out.push("declare ptr @mire_bool_to_string(i64)".to_string());
        out.push("declare ptr @mire_string_copy(ptr)".to_string());
        out.push("declare ptr @mire_string_concat(ptr, ptr)".to_string());
        out.push("declare ptr @mire_string_append_owned(ptr, ptr)".to_string());
        out.push("declare void @mire_string_free(ptr)".to_string());
        out.push("declare ptr @mire_string_to_upper(ptr)".to_string());
        out.push("declare ptr @mire_string_to_lower(ptr)".to_string());
        out.push("declare ptr @mire_strings_replace(ptr, ptr, ptr)".to_string());
        out.push("declare ptr @mire_strings_split(ptr, ptr)".to_string());
        out.push("declare ptr @mire_strings_join(ptr, i64, ptr)".to_string());
        out.push("declare ptr @mire_strings_trim(ptr)".to_string());
        out.push("declare ptr @mire_list_push_i64(ptr, i64)".to_string());
        out.push("declare ptr @mire_list_push_scalar(ptr, i64, i64)".to_string());
        out.push("declare ptr @mire_list_push_ptr(ptr, ptr)".to_string());
        out.push("declare ptr @mire_list_concat(ptr, ptr)".to_string());
        out.push("declare i64 @mire_dict_get_i64(ptr, i64, i64, ptr, i64)".to_string());
        out.push("declare ptr @mire_dict_get_ptr(ptr, i64, i64, ptr, ptr)".to_string());
        out.push("declare ptr @mire_dict_set_i64(ptr, i64, i64, i64, ptr, i64)".to_string());
        out.push("declare ptr @mire_dict_set_ptr(ptr, i64, i64, i64, ptr, ptr)".to_string());
        out.push("declare ptr @mire_dict_to_string(ptr)".to_string());
        out.push("declare ptr @mire_dict_keys(ptr)".to_string());
        out.push("declare ptr @mire_dict_values(ptr)".to_string());
        out.push("declare ptr @mire_list_slice(ptr, i64, i64)".to_string());
        out.push("declare ptr @fgets(ptr, i64, ptr)".to_string());
        out.push("@.fmt_i64 = private unnamed_addr constant [5 x i8] c\"%ld\\0A\\00\"".to_string());
        out.push("@.fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"".to_string());
        out.push(
            "@.fmt_float = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\"".to_string(),
        );
        out.push(
            "@.fmt_bool_true = private unnamed_addr constant [5 x i8] c\"true\\00\"".to_string(),
        );
        out.push(
            "@.fmt_bool_false = private unnamed_addr constant [6 x i8] c\"false\\00\"".to_string(),
        );
        out.push("@.fmt_i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\"".to_string());
        out.push("@.fmt_prompt = private unnamed_addr constant [3 x i8] c\"%s\\00\"".to_string());
        out.push("@.scanf_str = private unnamed_addr constant [3 x i8] c\"%s\\00\"".to_string());
        out.push("@.scanf_i64 = private unnamed_addr constant [4 x i8] c\"%ld\\00\"".to_string());
        out.extend(self.strings);
        out.push(String::new());
        let has_functions = !self.functions.is_empty();
        out.extend(self.functions);
        if has_functions {
            out.push(String::new());
        }
        out.push("define i32 @main() {".to_string());
        out.push("entry:".to_string());
        out.extend(self.entry_allocas);
        out.extend(self.body);
        out.push("}".to_string());
        out.push(String::new());
        Ok(out.join("\n"))
    }

    fn compile_statement(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::Use { .. } => Ok(()),
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
                Ok(())
            }
            Statement::Assignment { target, value, .. } => {
                let var = self.vars.get(target).cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Avenys does not know variable '{}'", target),
                    })
                })?;
                if self.try_compile_in_place_string_append(target, &var, value)? {
                    return Ok(());
                }
                let compiled = self.compile_expr(value)?;
                self.store_variable(target, &var.ptr, var.ty, var.data_type.clone(), compiled)?;
                let struct_name = self.struct_name_from_expr(value);
                if let Some(slot) = self.vars.get_mut(target) {
                    slot.struct_name = struct_name;
                }
                Ok(())
            }
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
                iterable,
                body,
            } => self.compile_for_range(variable, iterable, body),
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
            Statement::Expression(Expression::Call { name, args, .. })
                if name == "dasu" || name == "std.output" =>
            {
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
            other => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys does not yet lower statement {:?}", other),
            })),
        }
    }

    fn compile_expr(&mut self, expr: &Expression) -> Result<LlValue> {
        match expr {
            Expression::Literal(Literal::Int(value)) => Ok(LlValue {
                ty: LlType::I64,
                repr: value.to_string(),
                owned: false,
            }),
            Expression::Literal(Literal::Float(value)) => Ok(self.string_value(&value.to_string())),
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
            Expression::Literal(Literal::None) => Ok(LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            }),
            Expression::Identifier(Identifier { name, .. }) => {
                let var = self.vars.get(name).cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Avenys unknown identifier '{}'", name),
                    })
                })?;
                let tmp = self.tmp();
                let var_ty = var.ty.clone();
                self.body.push(format!(
                    "  {tmp} = load {}, ptr {}",
                    self.ty(var_ty.clone()),
                    var.ptr
                ));
                Ok(LlValue {
                    ty: var_ty,
                    repr: tmp,
                    owned: var.owns_heap_string,
                })
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
                let lhs = self.compile_expr(left)?;
                let rhs = self.compile_expr(right)?;

                let left_is_list = matches!(data_type, DataType::Vector { .. } | DataType::List);
                let right_is_list = matches!(data_type, DataType::Vector { .. } | DataType::List);

                if operator == "+" && left_is_list && right_is_list {
                    let result = self.tmp();
                    self.body.push(format!(
                        "  {result} = call ptr @mire_list_concat(ptr {}, ptr {})",
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
                            "  {tmp} = call ptr @mire_dict_to_string(ptr {})",
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
                            "  {tmp} = call ptr @mire_bool_to_string(i64 {})",
                            i64_value.repr
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
                                "  {tmp} = call ptr @mire_i64_to_string(i64 {})",
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
                                "  {tmp} = call ptr @mire_bool_to_string(i64 {})",
                                i64_value.repr
                            ));
                            Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            })
                        }
                        LlType::Struct(_) => Err(MireError::new(ErrorKind::Runtime {
                            message: "Avenys does not yet lower str(...) for struct values"
                                .to_string(),
                        })),
                    },
                }
            }
            Expression::Call { name, args, .. } if name == "len" => self.compile_len(args),
            Expression::Call { name, args, .. } if name == "dasu" || name == "std.output" => {
                for arg in args {
                    self.emit_dasu_expr(arg)?;
                }
                Ok(self.string_value(""))
            }
            Expression::Call {
                name,
                args,
                data_type,
            } if name == "ireru" || name == "std.input" => self.compile_input_expr(args, data_type),
            Expression::Call { name, args, .. } if name == "__if_expr" => {
                self.compile_if_expr(args)
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
                self.compile_index(target_val, index_val, &target_type, data_type)
            }
            Expression::MemberAccess { target, member, .. } => {
                self.compile_member_access(target, member)
            }
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
            Expression::Call { name, args, .. } if name == "strings.replace" => {
                self.compile_replace(args)
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
            Expression::Call { name, args, .. } if name == "strings.to_string" => {
                self.compile_to_string(args)
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
            Expression::Call { name, args, .. } if name == "time.mark" => {
                self.compile_time_mark(args)
            }
            Expression::Call { name, args, .. } if name == "time.elapsed_ms" => {
                self.compile_time_elapsed_ms(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.mark" => {
                self.compile_cpu_mark(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.elapsed_ms" => {
                self.compile_cpu_elapsed_ms(args)
            }
            Expression::Call { name, args, .. } if name == "cpu.cycles_est" => {
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
            Expression::Call { name, args, .. } if name == "lists.push" => {
                self.compile_lists_push(args)
            }
            Expression::Call { name, args, .. } if name == "math.sum" => {
                self.compile_math_sum(args)
            }
            Expression::Call { name, args, .. } if name == "strings.replace" => {
                self.compile_strings_replace(args)
            }
            Expression::Call {
                name,
                args,
                data_type,
            } => {
                // Check if this is a struct constructor call
                if let DataType::Struct = data_type {
                    return self.compile_struct_constructor(name, args);
                }

                let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!("Avenys does not yet lower call '{}'", name),
                    })
                })?;
                if fn_info.params.len() != args.len() {
                    return Err(MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Avenys function '{}' expects {} args, got {}",
                            name,
                            fn_info.params.len(),
                            args.len()
                        ),
                    }));
                }
                let mut rendered_args = Vec::with_capacity(args.len());
                for (arg_expr, expected_ty) in args.iter().zip(fn_info.params.iter()) {
                    let value = self.compile_expr(arg_expr)?;
                    let casted = match expected_ty {
                        LlType::I64 => self.cast_to_i64(value)?,
                        LlType::I1 => self.cast_to_i1(value)?,
                        LlType::Ptr if value.ty == LlType::Ptr => value,
                        LlType::Ptr => {
                            return Err(MireError::new(ErrorKind::Runtime {
                                message: format!(
                                    "Avenys cannot cast argument for function '{}'",
                                    name
                                ),
                            }))
                        }
                        LlType::Struct(_) => value,
                    };
                    let expected_ty = expected_ty.clone();
                    rendered_args.push(format!("{} {}", self.ty(expected_ty.clone()), casted.repr));
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
            Expression::Pipeline {
                input, stage, safe, ..
            } => {
                let input_val = self.compile_expr(input)?;

                match stage.as_ref() {
                    Expression::Call {
                        name,
                        args,
                        data_type: _,
                    } => {
                        if name == "len" {
                            return self.compile_pipeline_len(input, input_val);
                        }

                        let mut all_args = vec![input_val];
                        for arg in args {
                            let arg_val = self.compile_expr(arg)?;
                            all_args.push(arg_val);
                        }

                        if *safe {
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @mire_option_wrap(i64 {})",
                                all_args[1].repr
                            ));
                            return Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            });
                        }

                        let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
                            MireError::new(ErrorKind::Runtime {
                                message: format!("Avenys does not yet lower call '{}'", name),
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

                        let all_args = vec![input_val];

                        if *safe {
                            let tmp = self.tmp();
                            self.body.push(format!(
                                "  {tmp} = call ptr @mire_option_wrap(i64 {})",
                                all_args[0].repr
                            ));
                            return Ok(LlValue {
                                ty: LlType::Ptr,
                                repr: tmp,
                                owned: true,
                            });
                        }

                        let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
                            MireError::new(ErrorKind::Runtime {
                                message: format!("Avenys does not yet lower call '{}'", name),
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
                    _ => Err(MireError::new(ErrorKind::Runtime {
                        message: "Pipeline stage must be a function call or identifier".to_string(),
                    })),
                }
            }
            other => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys does not yet lower expression {:?}", other),
            })),
        }
    }

    fn compile_list_len(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys lists.len expects 1 argument".to_string(),
            }));
        }
        let list = self.compile_expr(&args[0])?;
        self.compile_list_len_value(list)
    }

    fn compile_list_len_value(&mut self, list: LlValue) -> Result<LlValue> {
        let list_ptr = self.tmp();
        self.body.push(format!(
            "  {list_ptr} = getelementptr inbounds i8, ptr {}, i64 -8",
            list.repr
        ));

        let is_null = self.tmp();
        let loaded_len = self.tmp();
        let len = self.tmp();
        let null_label = self.label("list_len_null");
        let load_label = self.label("list_len_load");
        let end_label = self.label("list_len_end");
        let result_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {result_ptr} = alloca i64"));

        self.body
            .push(format!("  {is_null} = icmp eq ptr {list_ptr}, null"));
        self.body.push(format!(
            "  br i1 {is_null}, label %{null_label}, label %{load_label}"
        ));

        self.body.push(format!("{null_label}:"));
        self.body.push(format!("  store i64 0, ptr {result_ptr}"));
        self.body.push(format!("  br label %{end_label}"));

        self.body.push(format!("{load_label}:"));
        self.body
            .push(format!("  {loaded_len} = load i64, ptr {list_ptr}"));
        self.body
            .push(format!("  store i64 {loaded_len}, ptr {result_ptr}"));
        self.body.push(format!("  br label %{end_label}"));

        self.body.push(format!("{end_label}:"));
        self.body
            .push(format!("  {len} = load i64, ptr {result_ptr}"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: len,
            owned: false,
        })
    }

    fn compile_pipeline_len(&mut self, input: &Expression, value: LlValue) -> Result<LlValue> {
        match self.expression_data_type(input) {
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
            DataType::List | DataType::Vector { .. } => self.compile_list_len_value(value),
            _ => match value.ty {
                LlType::Ptr => self.compile_list_len_value(value),
                LlType::I64 | LlType::I1 | LlType::Struct(_) => Ok(LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                }),
            },
        }
    }

    fn compile_list_get(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys lists.get expects 2 arguments".to_string(),
            }));
        }
        let list = self.compile_expr(&args[0])?;
        let index = self.compile_expr(&args[1])?;
        let list_type = self.expression_data_type(&args[0]);
        let elem_type = match &list_type {
            DataType::Vector { element_type, .. } => *element_type.clone(),
            DataType::Array { element_type, .. } => *element_type.clone(),
            DataType::Slice { element_type } => *element_type.clone(),
            _ => DataType::I64,
        };
        self.compile_index(list, index, &list_type, &elem_type)
    }

    fn compile_member_access(&mut self, target: &Expression, member: &str) -> Result<LlValue> {
        let target_val = self.compile_expr(target)?;
        let struct_name = self.struct_name_from_expr(target).ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys cannot resolve struct member '{}' without concrete struct metadata",
                    member
                ),
            })
        })?;
        let struct_info = self
            .user_structs
            .get(&struct_name)
            .cloned()
            .ok_or_else(|| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Unknown struct '{}'", struct_name),
                })
            })?;
        let field_index = struct_info
            .field_indices
            .get(member)
            .copied()
            .ok_or_else(|| {
                MireError::new(ErrorKind::Runtime {
                    message: format!("Struct '{}' has no field '{}'", struct_name, member),
                })
            })?;
        let struct_ty = self.render_struct_ty(&struct_info.fields);
        let field_ptr = self.tmp();
        self.body.push(format!(
            "  {field_ptr} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
            struct_ty, target_val.repr, field_index
        ));
        let field_ty = struct_info
            .fields
            .get(field_index)
            .cloned()
            .unwrap_or(LlType::I64);

        match field_ty {
            LlType::I64 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load i64, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: value,
                    owned: false,
                })
            }
            LlType::I1 => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load i1, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: value,
                    owned: false,
                })
            }
            LlType::Ptr => {
                let value = self.tmp();
                self.body
                    .push(format!("  {value} = load ptr, ptr {field_ptr}"));
                Ok(LlValue {
                    ty: LlType::Ptr,
                    repr: value,
                    owned: false,
                })
            }
            LlType::Struct(_) => Err(MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Avenys does not yet lower nested struct member '{}.{}'",
                    struct_name, member
                ),
            })),
        }
    }

    fn compile_index(
        &mut self,
        target: LlValue,
        index: LlValue,
        target_data_type: &DataType,
        result_data_type: &DataType,
    ) -> Result<LlValue> {
        if target.ty != LlType::Ptr {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot index non-pointer type".to_string(),
            }));
        }

        match target_data_type {
            DataType::List
            | DataType::Vector { .. }
            | DataType::Array { .. }
            | DataType::Slice { .. }
            | DataType::Tuple => {
                let elem_size = self.element_size(result_data_type);
                let data_ptr = self.tmp();
                self.body
                    .push(format!("  {data_ptr} = bitcast ptr {} to ptr", target.repr));
                let offset = self.tmp();
                self.body.push(format!(
                    "  {offset} = mul i64 {}, {}",
                    index.repr, elem_size
                ));
                let elem_ptr = self.tmp();
                self.body.push(format!(
                    "  {elem_ptr} = getelementptr inbounds i8, ptr {data_ptr}, i64 {offset}"
                ));
                let elem_ty = self.map_type(result_data_type)?;
                if elem_ty == LlType::Ptr {
                    let val = self.tmp();
                    self.body
                        .push(format!("  {val} = load ptr, ptr {elem_ptr}"));
                    Ok(LlValue {
                        ty: LlType::Ptr,
                        repr: val,
                        owned: false,
                    })
                } else if elem_ty == LlType::I1 {
                    let raw = self.tmp();
                    let val = self.tmp();
                    self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                    self.body.push(format!("  {val} = icmp ne i8 {raw}, 0"));
                    Ok(LlValue {
                        ty: LlType::I1,
                        repr: val,
                        owned: false,
                    })
                } else {
                    let raw_ty = self.scalar_storage_ir_type(result_data_type);
                    let raw = self.tmp();
                    self.body
                        .push(format!("  {raw} = load {raw_ty}, ptr {elem_ptr}"));
                    let val = match raw_ty {
                        "i8" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U8) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i8 {raw} to i64"));
                            widened
                        }
                        "i16" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U16) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i16 {raw} to i64"));
                            widened
                        }
                        "i32" => {
                            let widened = self.tmp();
                            let ext = if matches!(result_data_type, DataType::U32) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.body
                                .push(format!("  {widened} = {ext} i32 {raw} to i64"));
                            widened
                        }
                        _ => raw,
                    };
                    Ok(LlValue {
                        ty: LlType::I64,
                        repr: val,
                        owned: false,
                    })
                }
            }
            DataType::Str => {
                let elem_ptr = self.tmp();
                self.body.push(format!(
                    "  {elem_ptr} = getelementptr inbounds i8, ptr {}, i64 {}",
                    target.repr, index.repr
                ));
                let byte = self.tmp();
                self.body
                    .push(format!("  {byte} = load i8, ptr {elem_ptr}"));
                let widened = self.tmp();
                self.body
                    .push(format!("  {widened} = zext i8 {byte} to i64"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: widened,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys cannot index type {:?}", target_data_type),
            })),
        }
    }

    fn compile_list_push(&mut self, args: &[Expression]) -> Result<LlValue> {
        self.compile_lists_push(args)
    }

    fn compile_list_pop(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys list.pop(...) expects 1 argument".to_string(),
            }));
        }
        Ok(LlValue {
            ty: LlType::I64,
            repr: "0".to_string(),
            owned: false,
        })
    }

    fn compile_dict_get(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 && args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys dict.get(...) expects 2 or 3 arguments".to_string(),
            }));
        }

        let (dict_key_type, dict_value_type) = match self.expression_data_type(&args[0]) {
            DataType::Map {
                key_type,
                value_type,
            } => (*key_type, *value_type),
            _ => (DataType::Unknown, DataType::I64),
        };
        let dict = self.compile_expr(&args[0])?;
        let key = self.compile_expr(&args[1])?;
        let key_kind = self.runtime_kind_code(&dict_key_type);
        let key_i64 = if key.ty == LlType::Ptr {
            LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            }
        } else {
            self.cast_to_i64(key.clone())?
        };
        let key_ptr = if key.ty == LlType::Ptr {
            key
        } else {
            LlValue {
                ty: LlType::Ptr,
                repr: "null".to_string(),
                owned: false,
            }
        };

        if matches!(
            dict_value_type,
            DataType::Map { .. }
                | DataType::Vector { .. }
                | DataType::Array { .. }
                | DataType::Slice { .. }
                | DataType::Str
        ) {
            let default_value = if args.len() == 3 {
                let value = self.compile_expr(&args[2])?;
                self.cast_to_type(value, LlType::Ptr)?
            } else {
                LlValue {
                    ty: LlType::Ptr,
                    repr: "null".to_string(),
                    owned: false,
                }
            };
            let result = self.tmp();
            self.body.push(format!(
                "  {result} = call ptr @mire_dict_get_ptr(ptr {}, i64 {}, i64 {}, ptr {}, ptr {})",
                dict.repr, key_kind, key_i64.repr, key_ptr.repr, default_value.repr
            ));
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: result,
                owned: false,
            });
        }

        let default_value = if args.len() == 3 {
            let value = self.compile_expr(&args[2])?;
            self.cast_to_i64(value)?
        } else {
            LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            }
        };
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call i64 @mire_dict_get_i64(ptr {}, i64 {}, i64 {}, ptr {}, i64 {})",
            dict.repr, key_kind, key_i64.repr, key_ptr.repr, default_value.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    fn compile_dict_set(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys dict.set(...) expects 3 arguments".to_string(),
            }));
        }
        let dict_type = self.expression_data_type(&args[0]);
        let (key_data_type, value_data_type) = match dict_type {
            DataType::Map {
                key_type,
                value_type,
            } => (*key_type, *value_type),
            _ => (
                self.expression_data_type(&args[1]),
                self.expression_data_type(&args[2]),
            ),
        };
        let dict = self.compile_expr(&args[0])?;
        let key = self.compile_expr(&args[1])?;
        let value_expr = self.compile_expr(&args[2])?;
        let key_kind = self.runtime_kind_code(&key_data_type);
        let value_kind = self.runtime_kind_code(&value_data_type);
        let key_i64 = if key.ty == LlType::Ptr {
            LlValue {
                ty: LlType::I64,
                repr: "0".to_string(),
                owned: false,
            }
        } else {
            self.cast_to_i64(key.clone())?
        };
        let key_ptr = if key.ty == LlType::Ptr {
            key
        } else {
            LlValue {
                ty: LlType::Ptr,
                repr: "null".to_string(),
                owned: false,
            }
        };
        let result = self.tmp();

        if value_expr.ty == LlType::Ptr {
            let value = self.cast_to_type(value_expr, LlType::Ptr)?;
            self.body.push(format!(
                "  {result} = call ptr @mire_dict_set_ptr(ptr {}, i64 {}, i64 {}, i64 {}, ptr {}, ptr {})",
                dict.repr, key_kind, value_kind, key_i64.repr, key_ptr.repr, value.repr
            ));
        } else {
            let value = self.cast_to_i64(value_expr)?;
            self.body.push(format!(
                "  {result} = call ptr @mire_dict_set_i64(ptr {}, i64 {}, i64 {}, i64 {}, ptr {}, i64 {})",
                dict.repr, key_kind, value_kind, key_i64.repr, key_ptr.repr, value.repr
            ));
        }
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_contains(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys contains(...) expects 2 arguments".to_string(),
            }));
        }
        Ok(LlValue {
            ty: LlType::I1,
            repr: "0".to_string(),
            owned: false,
        })
    }

    fn compile_dict_keys(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "dicts.keys(...) expects 1 argument".to_string(),
            }));
        }
        let dict = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_dict_keys(ptr {})",
            dict.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_dict_values(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "dicts.values(...) expects 1 argument".to_string(),
            }));
        }
        let dict = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_dict_values(ptr {})",
            dict.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_float(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys float(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_i64(value)
    }

    fn compile_int(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys int(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_i64(value)
    }

    fn compile_bool(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys bool(...) expects 1 argument".to_string(),
            }));
        }
        let value = self.compile_expr(&args[0])?;
        self.cast_to_i1(value)
    }

    fn compile_concat(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() < 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys concat(...) expects at least 2 arguments".to_string(),
            }));
        }

        let mut iter = args.iter().filter(
            |arg| !matches!(arg, Expression::Literal(Literal::Str(value)) if value.is_empty()),
        );

        let Some(first) = iter.next() else {
            return Ok(self.string_value(""));
        };

        let mut acc = self.compile_expr(first)?;
        for arg in iter {
            let value = self.compile_expr(arg)?;
            acc = self.concat_values(acc, value);
        }
        Ok(acc)
    }

    fn compile_replace(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys replace(...) expects 3 arguments".to_string(),
            }));
        }

        if let (
            Expression::Literal(Literal::Str(input)),
            Expression::Literal(Literal::Str(from)),
            Expression::Literal(Literal::Str(to)),
        ) = (&args[0], &args[1], &args[2])
        {
            return Ok(self.string_value(&input.replace(from, to)));
        }

        if let (_, Expression::Literal(Literal::Str(from)), Expression::Literal(Literal::Str(to))) =
            (&args[0], &args[1], &args[2])
        {
            if from.is_empty() || from == to {
                return self.compile_expr(&args[0]);
            }
        }

        let input = self.compile_expr(&args[0])?;
        let from = self.compile_expr(&args[1])?;
        let to = self.compile_expr(&args[2])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_strings_replace(ptr {}, ptr {}, ptr {})",
            input.repr, from.repr, to.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_split(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.split(...) expects 2 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let delimiter = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_strings_split(ptr {}, ptr {})",
            input.repr, delimiter.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_join(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.join(...) expects 2 arguments".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let delimiter = self.compile_expr(&args[1])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_strings_join(ptr {}, i64 0, ptr {})",
            input.repr, delimiter.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_trim(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.trim(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(input.trim()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_strings_trim(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_to_upper(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys to_upper(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(&input.to_ascii_uppercase()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_string_to_upper(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_to_lower(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys to_lower(...) expects 1 argument".to_string(),
            }));
        }
        if let Expression::Literal(Literal::Str(input)) = &args[0] {
            return Ok(self.string_value(&input.to_ascii_lowercase()));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_string_to_lower(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_to_string(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "strings.to_string(...) expects 1 argument".to_string(),
            }));
        }
        let input = self.compile_expr(&args[0])?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_dict_to_string(ptr {})",
            input.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_abs(&mut self, args: &[Expression]) -> Result<LlValue> {
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

    fn compile_sqrt(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys sqrt(...) expects 1 argument".to_string(),
            }));
        }
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: "null".to_string(),
            owned: false,
        })
    }

    fn compile_pow(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys pow(...) expects 2 arguments".to_string(),
            }));
        }
        let base = self.compile_expr(&args[0])?;
        let exp = self.compile_expr(&args[1])?;
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = call i64 @pow(i64 {}, i64 {})",
            base.repr, exp.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    fn compile_floor(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys floor(...) expects 1 argument".to_string(),
            }));
        }
        self.compile_expr(&args[0])
    }

    fn compile_ceil(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys ceil(...) expects 1 argument".to_string(),
            }));
        }
        self.compile_expr(&args[0])
    }

    fn compile_round(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys round(...) expects 1 argument".to_string(),
            }));
        }
        self.compile_expr(&args[0])
    }

    fn compile_min(&mut self, args: &[Expression]) -> Result<LlValue> {
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

    fn compile_max(&mut self, args: &[Expression]) -> Result<LlValue> {
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

    fn compile_range(&mut self, _args: &[Expression]) -> Result<LlValue> {
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: "null".to_string(),
            owned: false,
        })
    }

    fn compile_sleep(&mut self, args: &[Expression]) -> Result<LlValue> {
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

    fn compile_exit(&mut self, args: &[Expression]) -> Result<LlValue> {
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

    fn compile_time_mark(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let tmp = self.tmp();
        self.body
            .push(format!("  {tmp} = call i64 @mire_wall_mark_ns()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: tmp,
            owned: false,
        })
    }

    fn compile_time_elapsed_ms(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys time.elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call ptr @mire_wall_elapsed_ms_str(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: diff,
            owned: true,
        })
    }

    fn compile_cpu_mark(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call i64 @mire_cpu_mark_ns()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    fn compile_cpu_elapsed_ms(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cpu.elapsed_ms expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call ptr @mire_cpu_elapsed_ms_str(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: diff,
            owned: true,
        })
    }

    fn compile_cpu_cycles_est(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cpu.cycles_est expects 1 argument".to_string(),
            }));
        }
        let start = self.compile_expr(&args[0])?;
        let diff = self.tmp();
        self.body.push(format!(
            "  {diff} = call i64 @mire_cpu_cycles_est(i64 {})",
            start.repr
        ));
        Ok(LlValue {
            ty: LlType::I64,
            repr: diff,
            owned: false,
        })
    }

    fn compile_gpu_snapshot(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call ptr @mire_gpu_snapshot()"));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_mem_format(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys mem.format expects 1 argument".to_string(),
            }));
        }
        let value_expr = self.compile_expr(&args[0])?;
        let value = self.cast_to_i64(value_expr)?;
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_mem_format(i64 {})",
            value.repr
        ));
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_mem_process(&mut self, _args: &[Expression]) -> Result<LlValue> {
        let result = self.tmp();
        self.body
            .push(format!("  {result} = call i64 @mire_mem_process_bytes()"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    fn compile_lists_push(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys lists.push expects 2 arguments".to_string(),
            }));
        }

        let list = self.compile_expr(&args[0])?;
        let value = self.compile_expr(&args[1])?;
        let list_type = self.expression_data_type(&args[0]);
        let elem_type = match &list_type {
            DataType::Vector { element_type, .. } => *element_type.clone(),
            DataType::Array { element_type, .. } => *element_type.clone(),
            DataType::Slice { element_type } => *element_type.clone(),
            _ => DataType::I64,
        };
        let result = self.tmp();
        if value.ty == LlType::Ptr {
            self.body.push(format!(
                "  {result} = call ptr @mire_list_push_ptr(ptr {}, ptr {})",
                list.repr, value.repr
            ));
        } else {
            let value = self.cast_to_i64(value)?;
            let elem_size = self.element_size(&elem_type);
            if elem_size == 8 {
                self.body.push(format!(
                    "  {result} = call ptr @mire_list_push_i64(ptr {}, i64 {})",
                    list.repr, value.repr
                ));
            } else {
                self.body.push(format!(
                    "  {result} = call ptr @mire_list_push_scalar(ptr {}, i64 {}, i64 {})",
                    list.repr, value.repr, elem_size
                ));
            }
        }

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: false,
        })
    }

    fn compile_lists_slice(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys lists.slice expects 3 arguments".to_string(),
            }));
        }

        let list = self.compile_expr(&args[0])?;
        let start = self.compile_expr(&args[1])?;
        let end = self.compile_expr(&args[2])?;

        let start_i64 = self.cast_to_i64(start)?;
        let end_i64 = self.cast_to_i64(end)?;

        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_list_slice(ptr {}, i64 {}, i64 {})",
            list.repr, start_i64.repr, end_i64.repr
        ));

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        })
    }

    fn compile_strings_replace(&mut self, args: &[Expression]) -> Result<LlValue> {
        self.compile_replace(args)
    }

    fn compile_struct_constructor(
        &mut self,
        type_name: &str,
        args: &[Expression],
    ) -> Result<LlValue> {
        let struct_info = self.user_structs.get(type_name).cloned().ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Unknown struct '{}'", type_name),
            })
        })?;

        let ptr = self.tmp();
        self.body.push(format!(
            "  {ptr} = call ptr @malloc(i64 {})",
            struct_info.fields.len() * 8
        ));

        let struct_ty = self.render_struct_ty(&struct_info.fields);

        for arg in args {
            if let Expression::NamedArg { name, value, .. } = arg {
                if let Some(field_index) = struct_info.field_indices.get(name) {
                    let field_value = self.compile_expr(value)?;
                    let field_ptr = self.tmp();
                    self.body.push(format!(
                        "  {field_ptr} = getelementptr inbounds {}, ptr {ptr}, i32 0, i32 {}",
                        struct_ty, field_index
                    ));

                    let field_type = struct_info
                        .fields
                        .get(*field_index)
                        .cloned()
                        .unwrap_or(LlType::I64);
                    match field_type {
                        LlType::I64 => {
                            let casted = self.cast_to_i64(field_value)?;
                            self.body
                                .push(format!("  store i64 {}, ptr {field_ptr}", casted.repr));
                        }
                        LlType::I1 => {
                            let casted = self.cast_to_i1(field_value)?;
                            self.body
                                .push(format!("  store i1 {}, ptr {field_ptr}", casted.repr));
                        }
                        LlType::Ptr => {
                            self.body
                                .push(format!("  store ptr {}, ptr {field_ptr}", field_value.repr));
                        }
                        LlType::Struct(_) => {
                            self.body.push(format!(
                                "  store {}, ptr {field_ptr}",
                                self.ty(field_type.clone())
                            ));
                        }
                    }
                }
            }
        }

        Ok(LlValue {
            ty: LlType::Ptr,
            repr: ptr,
            owned: true,
        })
    }

    fn compile_len(&mut self, args: &[Expression]) -> Result<LlValue> {
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
            | Expression::Match { data_type, .. } => data_type,
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
                LlType::I64 | LlType::I1 | LlType::Struct(_) => Ok(LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                }),
            },
        }
    }

    fn compile_math_sum(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys math.sum expects 1 argument".to_string(),
            }));
        }

        let list = self.compile_expr(&args[0])?;
        let list_type = self.expression_data_type(&args[0]);
        let elem_type = match &list_type {
            DataType::Vector { element_type, .. } => *element_type.clone(),
            DataType::Array { element_type, .. } => *element_type.clone(),
            DataType::Slice { element_type } => *element_type.clone(),
            _ => DataType::I64,
        };
        let elem_size = self.element_size(&elem_type);
        let result_ptr = self.tmp();
        let index_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {result_ptr} = alloca i64"));
        self.entry_allocas
            .push(format!("  {index_ptr} = alloca i64"));
        self.body.push(format!("  store i64 0, ptr {result_ptr}"));
        self.body.push(format!("  store i64 0, ptr {index_ptr}"));

        let is_null = self.tmp();
        let null_label = self.label("math_sum_null");
        let loop_cond_label = self.label("math_sum_cond");
        let loop_body_label = self.label("math_sum_body");
        let end_label = self.label("math_sum_end");
        self.body
            .push(format!("  {is_null} = icmp eq ptr {}, null", list.repr));
        self.body.push(format!(
            "  br i1 {is_null}, label %{null_label}, label %{loop_cond_label}"
        ));

        self.body.push(format!("{null_label}:"));
        self.body.push(format!("  br label %{end_label}"));

        let len = self.tmp();
        let index = self.tmp();
        let has_more = self.tmp();
        self.body.push(format!("{loop_cond_label}:"));
        self.body
            .push(format!("  {len} = load i64, ptr {}", list.repr));
        self.body
            .push(format!("  {index} = load i64, ptr {index_ptr}"));
        self.body
            .push(format!("  {has_more} = icmp slt i64 {index}, {len}"));
        self.body.push(format!(
            "  br i1 {has_more}, label %{loop_body_label}, label %{end_label}"
        ));

        self.body.push(format!("{loop_body_label}:"));
        let data_ptr = self.tmp();
        let offset = self.tmp();
        let elem_ptr = self.tmp();
        let elem = self.tmp();
        let current_sum = self.tmp();
        let next_sum = self.tmp();
        let next_index = self.tmp();
        self.body.push(format!(
            "  {data_ptr} = getelementptr i8, ptr {}, i64 8",
            list.repr
        ));
        self.body
            .push(format!("  {offset} = mul i64 {index}, {}", elem_size));
        self.body.push(format!(
            "  {elem_ptr} = getelementptr i8, ptr {data_ptr}, i64 {offset}"
        ));
        match self.scalar_storage_ir_type(&elem_type) {
            "i8" => {
                let raw = self.tmp();
                let ext = if matches!(elem_type, DataType::U8) {
                    "zext"
                } else {
                    "sext"
                };
                self.body.push(format!("  {raw} = load i8, ptr {elem_ptr}"));
                self.body.push(format!("  {elem} = {ext} i8 {raw} to i64"));
            }
            "i16" => {
                let raw = self.tmp();
                let ext = if matches!(elem_type, DataType::U16) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {raw} = load i16, ptr {elem_ptr}"));
                self.body.push(format!("  {elem} = {ext} i16 {raw} to i64"));
            }
            "i32" => {
                let raw = self.tmp();
                let ext = if matches!(elem_type, DataType::U32) {
                    "zext"
                } else {
                    "sext"
                };
                self.body
                    .push(format!("  {raw} = load i32, ptr {elem_ptr}"));
                self.body.push(format!("  {elem} = {ext} i32 {raw} to i64"));
            }
            _ => {
                self.body
                    .push(format!("  {elem} = load i64, ptr {elem_ptr}"));
            }
        }
        self.body
            .push(format!("  {current_sum} = load i64, ptr {result_ptr}"));
        self.body
            .push(format!("  {next_sum} = add i64 {current_sum}, {elem}"));
        self.body
            .push(format!("  store i64 {next_sum}, ptr {result_ptr}"));
        self.body
            .push(format!("  {next_index} = add i64 {index}, 1"));
        self.body
            .push(format!("  store i64 {next_index}, ptr {index_ptr}"));
        self.body.push(format!("  br label %{loop_cond_label}"));

        self.body.push(format!("{end_label}:"));
        let result = self.tmp();
        self.body
            .push(format!("  {result} = load i64, ptr {result_ptr}"));
        Ok(LlValue {
            ty: LlType::I64,
            repr: result,
            owned: false,
        })
    }

    fn compile_if_expr(&mut self, args: &[Expression]) -> Result<LlValue> {
        if args.len() != 3 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys __if_expr expects 3 arguments".to_string(),
            }));
        }
        let then_expr = self.closure_return_expr(&args[1], "__if_expr then")?;
        let else_expr = self.closure_return_expr(&args[2], "__if_expr else")?;
        let then_value = self.compile_expr(then_expr)?;
        let result_ty = then_value.ty.clone();
        let result_ptr = self.tmp();
        let result_ty_clone = result_ty.clone();
        self.entry_allocas.push(format!(
            "  {result_ptr} = alloca {}",
            self.ty(result_ty_clone)
        ));

        let then_label = self.label("ifexpr_then");
        let else_label = self.label("ifexpr_else");
        let end_label = self.label("ifexpr_end");
        let cond_val = self.compile_expr(&args[0])?;
        let cond = self.cast_to_i1(cond_val)?;
        self.body.push(format!(
            "  br i1 {}, label %{then_label}, label %{else_label}",
            cond.repr
        ));

        self.body.push(format!("{then_label}:"));
        self.store_casted(&result_ptr, result_ty.clone(), then_value)?;
        self.body.push(format!("  br label %{end_label}"));

        self.body.push(format!("{else_label}:"));
        let else_value = self.compile_expr(else_expr)?;
        self.store_casted(&result_ptr, result_ty.clone(), else_value)?;
        self.body.push(format!("  br label %{end_label}"));

        self.body.push(format!("{end_label}:"));
        let loaded = self.tmp();
        self.body.push(format!(
            "  {loaded} = load {}, ptr {}",
            self.ty(result_ty.clone()),
            result_ptr
        ));
        Ok(LlValue {
            ty: result_ty,
            repr: loaded,
            owned: false,
        })
    }

    fn compile_match_statement(
        &mut self,
        value: &Expression,
        cases: &[(Expression, Vec<Statement>)],
        default: &[Statement],
    ) -> Result<()> {
        let match_value = self.compile_expr(value)?;
        let end_label = self.label("match_end");
        let default_label = self.label("match_default");
        let mut next_label = None;

        for (index, (pattern, body)) in cases.iter().enumerate() {
            let check_label = next_label
                .take()
                .unwrap_or_else(|| self.label("match_check"));
            let body_label = self.label(&format!("match_body_{index}"));
            let fallthrough_label = if index + 1 == cases.len() {
                default_label.clone()
            } else {
                self.label(&format!("match_next_{index}"))
            };

            if index > 0 {
                self.body.push(format!("{check_label}:"));
            }

            let cond = self.compile_match_case_condition(&match_value, pattern)?;
            self.body.push(format!(
                "  br i1 {}, label %{body_label}, label %{fallthrough_label}",
                cond.repr
            ));

            self.body.push(format!("{body_label}:"));
            for stmt in body {
                self.compile_statement(stmt)?;
            }
            self.body.push(format!("  br label %{end_label}"));
            next_label = Some(fallthrough_label);
        }

        let default_entry = next_label.unwrap_or(default_label.clone());
        self.body.push(format!("{default_entry}:"));
        for stmt in default {
            self.compile_statement(stmt)?;
        }
        self.body.push(format!("  br label %{end_label}"));
        self.body.push(format!("{end_label}:"));
        Ok(())
    }

    fn compile_match_expr(
        &mut self,
        value: &Expression,
        cases: &[(Expression, Expression)],
        default: &Expression,
        data_type: &DataType,
    ) -> Result<LlValue> {
        let match_value = self.compile_expr(value)?;
        let result_ty = self.map_type(data_type)?;
        let result_ptr = self.tmp();
        self.entry_allocas.push(format!(
            "  {result_ptr} = alloca {}",
            self.ty(result_ty.clone())
        ));

        let end_label = self.label("match_expr_end");
        let default_label = self.label("match_expr_default");
        let mut next_label = None;

        for (index, (pattern, result_expr)) in cases.iter().enumerate() {
            let check_label = next_label
                .take()
                .unwrap_or_else(|| self.label("match_expr_check"));
            let body_label = self.label(&format!("match_expr_body_{index}"));
            let fallthrough_label = if index + 1 == cases.len() {
                default_label.clone()
            } else {
                self.label(&format!("match_expr_next_{index}"))
            };

            if index > 0 {
                self.body.push(format!("{check_label}:"));
            }

            let cond = self.compile_match_case_condition(&match_value, pattern)?;
            self.body.push(format!(
                "  br i1 {}, label %{body_label}, label %{fallthrough_label}",
                cond.repr
            ));

            self.body.push(format!("{body_label}:"));
            let body_value = self.compile_expr(result_expr)?;
            self.store_casted(&result_ptr, result_ty.clone(), body_value)?;
            self.body.push(format!("  br label %{end_label}"));
            next_label = Some(fallthrough_label);
        }

        let default_entry = next_label.unwrap_or(default_label.clone());
        self.body.push(format!("{default_entry}:"));

        // Handle default case - if it's a wildcard _, use a default value
        if let Expression::Identifier(ident) = default {
            if ident.name == "_" {
                // Default case - just set result to 0 or default for the type
                let default_val = self.default_value(result_ty.clone());
                self.store_casted(&result_ptr, result_ty.clone(), default_val)?;
            } else {
                let default_value = self.compile_expr(default)?;
                self.store_casted(&result_ptr, result_ty.clone(), default_value)?;
            }
        } else {
            let default_value = self.compile_expr(default)?;
            self.store_casted(&result_ptr, result_ty.clone(), default_value)?;
        }
        self.body.push(format!("  br label %{end_label}"));

        self.body.push(format!("{end_label}:"));
        let loaded = self.tmp();
        self.body.push(format!(
            "  {loaded} = load {}, ptr {}",
            self.ty(result_ty.clone()),
            result_ptr
        ));
        Ok(LlValue {
            ty: result_ty,
            repr: loaded,
            owned: false,
        })
    }

    fn compile_match_case_condition(
        &mut self,
        value: &LlValue,
        pattern: &Expression,
    ) -> Result<LlValue> {
        // Handle wildcard pattern - always matches (true)
        if let Expression::Identifier(ident) = pattern {
            if ident.name == "_" {
                let result = self.tmp();
                self.body.push(format!("  {result} = add i1 0, 1"));
                return Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                });
            }
        }

        let pattern_value = self.compile_expr(pattern)?;
        let result = self.tmp();

        match (&value.ty, &pattern_value.ty) {
            (LlType::Ptr, LlType::Ptr) => {
                let cmp_value = self.tmp();
                self.body.push(format!(
                    "  {cmp_value} = call i32 @strcmp(ptr {}, ptr {})",
                    value.repr, pattern_value.repr
                ));
                self.body
                    .push(format!("  {result} = icmp eq i32 {cmp_value}, 0"));
            }
            (LlType::I1, LlType::I1) => {
                self.body.push(format!(
                    "  {result} = icmp eq i1 {}, {}",
                    value.repr, pattern_value.repr
                ));
            }
            (LlType::I64, LlType::I64) => {
                self.body.push(format!(
                    "  {result} = icmp eq i64 {}, {}",
                    value.repr, pattern_value.repr
                ));
            }
            (LlType::I1, LlType::I64)
            | (LlType::I64, LlType::I1)
            | (LlType::I64, LlType::Ptr)
            | (LlType::Ptr, LlType::I64) => {
                let lhs = self.cast_to_i64(value.clone())?;
                let rhs = self.cast_to_i64(pattern_value)?;
                self.body.push(format!(
                    "  {result} = icmp eq i64 {}, {}",
                    lhs.repr, rhs.repr
                ));
            }
            _ => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Avenys does not yet compare match values of type {:?} against {:?}",
                        value.ty, pattern_value.ty
                    ),
                }))
            }
        }

        Ok(LlValue {
            ty: LlType::I1,
            repr: result,
            owned: false,
        })
    }

    fn compile_do_while(&mut self, args: &[Expression]) -> Result<()> {
        if args.len() != 2 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys __do_while expects 2 closures".to_string(),
            }));
        }
        let body = self.closure_statements(&args[0], "__do_while body")?;
        let condition = self.closure_return_expr(&args[1], "__do_while condition")?;

        let body_label = self.label("dowhile_body");
        let cond_label = self.label("dowhile_cond");
        let end_label = self.label("dowhile_end");

        self.body.push(format!("  br label %{body_label}"));
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

        self.body.push(format!("{cond_label}:"));
        let cond_val = self.compile_expr(condition)?;
        let cond = self.cast_to_i1(cond_val)?;
        self.body.push(format!(
            "  br i1 {}, label %{body_label}, label %{end_label}",
            cond.repr
        ));
        self.body.push(format!("{end_label}:"));
        Ok(())
    }

    fn compile_for_range(
        &mut self,
        variable: &str,
        iterable: &Expression,
        body: &[Statement],
    ) -> Result<()> {
        let (start_expr, end_expr, step_expr) = match iterable {
            Expression::Call { name, args, .. } if name == "range" => match args.len() {
                1 => (
                    Expression::Literal(Literal::Int(0)),
                    args[0].clone(),
                    Expression::Literal(Literal::Int(1)),
                ),
                2 => (
                    args[0].clone(),
                    args[1].clone(),
                    Expression::Literal(Literal::Int(1)),
                ),
                3 => (args[0].clone(), args[1].clone(), args[2].clone()),
                _ => {
                    return Err(MireError::new(ErrorKind::Runtime {
                        message: "Avenys range(...) supports 1 to 3 arguments".to_string(),
                    }))
                }
            },
            other => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Avenys for-loop currently supports range(...) only, found {:?}",
                        other
                    ),
                }))
            }
        };

        let start_value = self.compile_expr(&start_expr)?;
        let start = self.cast_to_i64(start_value)?;
        let end_value = self.compile_expr(&end_expr)?;
        let end = self.cast_to_i64(end_value)?;
        let step_value = self.compile_expr(&step_expr)?;
        let step = self.cast_to_i64(step_value)?;
        let iter_ptr = self.tmp();
        self.entry_allocas
            .push(format!("  {iter_ptr} = alloca i64"));
        self.body
            .push(format!("  store i64 {}, ptr {}", start.repr, iter_ptr));

        let saved = self.vars.insert(
            variable.to_string(),
            VarInfo {
                ptr: iter_ptr.clone(),
                ty: LlType::I64,
                data_type: DataType::I64,
                owns_heap_string: false,
                struct_name: None,
            },
        );

        let cond_label = self.label("for_cond");
        let body_label = self.label("for_body");
        let continue_label = self.label("for_continue");
        let positive_label = self.label("for_positive");
        let negative_label = self.label("for_negative");
        let cond_merge_label = self.label("for_cond_merge");
        let end_label = self.label("for_end");
        let step_positive = self.tmp();
        let current_val = self.tmp();
        let pos_cmp = self.tmp();
        let neg_cmp = self.tmp();
        let cmp_ptr = self.tmp();
        self.entry_allocas.push(format!("  {cmp_ptr} = alloca i1"));

        self.body.push(format!("  br label %{cond_label}"));
        self.body.push(format!("{cond_label}:"));
        self.body
            .push(format!("  {step_positive} = icmp sgt i64 {}, 0", step.repr));
        self.body
            .push(format!("  {current_val} = load i64, ptr {}", iter_ptr));
        self.body.push(format!(
            "  br i1 {}, label %{positive_label}, label %{negative_label}",
            step_positive
        ));
        self.body.push(format!("{positive_label}:"));
        self.body.push(format!(
            "  {pos_cmp} = icmp slt i64 {}, {}",
            current_val, end.repr
        ));
        self.body
            .push(format!("  store i1 {}, ptr {}", pos_cmp, cmp_ptr));
        self.body.push(format!("  br label %{cond_merge_label}"));
        self.body.push(format!("{negative_label}:"));
        self.body.push(format!(
            "  {neg_cmp} = icmp sgt i64 {}, {}",
            current_val, end.repr
        ));
        self.body
            .push(format!("  store i1 {}, ptr {}", neg_cmp, cmp_ptr));
        self.body.push(format!("  br label %{cond_merge_label}"));
        self.body.push(format!("{cond_merge_label}:"));
        let cmp_tmp = self.tmp();
        self.body
            .push(format!("  {cmp_tmp} = load i1, ptr {}", cmp_ptr));
        self.body.push(format!(
            "  br i1 {}, label %{body_label}, label %{end_label}",
            cmp_tmp
        ));

        self.body.push(format!("{body_label}:"));
        self.loop_stack.push(LoopLabels {
            break_label: end_label.clone(),
            continue_label: continue_label.clone(),
        });
        for stmt in body {
            self.compile_statement(stmt)?;
        }
        self.loop_stack.pop();
        self.body.push(format!("  br label %{continue_label}"));

        self.body.push(format!("{continue_label}:"));
        let iter_value = self.tmp();
        let next_value = self.tmp();
        self.body
            .push(format!("  {iter_value} = load i64, ptr {}", iter_ptr));
        self.body.push(format!(
            "  {next_value} = add i64 {}, {}",
            iter_value, step.repr
        ));
        self.body
            .push(format!("  store i64 {}, ptr {}", next_value, iter_ptr));
        self.body.push(format!("  br label %{cond_label}"));
        self.body.push(format!("{end_label}:"));

        if let Some(saved) = saved {
            self.vars.insert(variable.to_string(), saved);
        } else {
            self.vars.remove(variable);
        }

        Ok(())
    }

    fn closure_statements<'a>(&self, expr: &'a Expression, ctx: &str) -> Result<&'a [Statement]> {
        match expr {
            Expression::Closure { params, body, .. } if params.is_empty() => Ok(body),
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys expects a zero-arg closure for {}", ctx),
            })),
        }
    }

    fn closure_return_expr<'a>(&self, expr: &'a Expression, ctx: &str) -> Result<&'a Expression> {
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

    fn emit_print(&mut self, value: &LlValue) -> Result<()> {
        match value.ty {
            LlType::I64 => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_i64, i64 {})",
                    value.repr
                ));
                Ok(())
            }
            LlType::Ptr => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                    value.repr
                ));
                Ok(())
            }
            LlType::I1 => {
                let true_ptr = self.string_value("true");
                let false_ptr = self.string_value("false");
                let select = self.tmp();
                self.body.push(format!(
                    "  {select} = select i1 {}, ptr {}, ptr {}",
                    value.repr, true_ptr.repr, false_ptr.repr
                ));
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {select})"
                ));
                Ok(())
            }
            LlType::Struct(_) => {
                self.body.push(format!(
                    "  call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                    value.repr
                ));
                Ok(())
            }
        }
    }

    fn emit_dasu_expr(&mut self, expr: &Expression) -> Result<()> {
        let value = self.compile_expr(expr)?;
        self.emit_print(&value)?;
        Ok(())
    }

    fn struct_name_from_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Call {
                name, data_type, ..
            } if *data_type == DataType::Struct && self.user_structs.contains_key(name) => {
                Some(name.clone())
            }
            Expression::Identifier(Identifier { name, .. }) => self
                .vars
                .get(name)
                .and_then(|info| info.struct_name.clone()),
            _ => None,
        }
    }

    fn compile_input_expr(&mut self, args: &[Expression], data_type: &DataType) -> Result<LlValue> {
        if args.len() != 1 {
            return Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys input expects 1 argument".to_string(),
            }));
        }

        let prompt = self.compile_expr(&args[0])?;
        self.body.push(format!(
            "  call i32 (ptr, ...) @printf(ptr @.fmt_prompt, ptr {})",
            prompt.repr
        ));

        match data_type {
            DataType::I64 | DataType::I32 | DataType::I16 | DataType::I8 => {
                let temp_buf = self.tmp();
                let result = self.tmp();
                self.body.push(format!("  {temp_buf} = alloca i64"));
                self.body.push(format!(
                    "  call i32 (ptr, ...) @scanf(ptr @.scanf_i64, ptr {temp_buf})"
                ));
                self.body
                    .push(format!("  {result} = load i64, ptr {temp_buf}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            _ => {
                let malloc_result = self.tmp();
                let input_buf = self.tmp();
                self.body
                    .push(format!("  {malloc_result} = call i64 @malloc(i64 256)"));
                self.body.push(format!(
                    "  {input_buf} = inttoptr i64 {malloc_result} to ptr"
                ));
                self.body.push(format!(
                    "  call i32 (ptr, ...) @scanf(ptr @.scanf_str, ptr {input_buf})"
                ));
                Ok(LlValue {
                    ty: LlType::Ptr,
                    repr: input_buf,
                    owned: true,
                })
            }
        }
    }

    fn expression_data_type(&self, expr: &Expression) -> DataType {
        match expr {
            Expression::Literal(Literal::Str(_)) => DataType::Str,
            Expression::Literal(Literal::Bool(_)) => DataType::Bool,
            Expression::Literal(Literal::Int(_)) => DataType::I64,
            Expression::Literal(Literal::List(_)) => DataType::Vector {
                element_type: Box::new(DataType::Unknown),
                dynamic: false,
            },
            Expression::Literal(Literal::Dict(_)) => DataType::Map {
                key_type: Box::new(DataType::Unknown),
                value_type: Box::new(DataType::Unknown),
            },
            Expression::Literal(_) => DataType::Unknown,
            Expression::Identifier(identifier) => identifier.data_type.clone(),
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
            | Expression::Match { data_type, .. } => data_type.clone(),
            Expression::Closure { return_type, .. } => return_type.clone(),
        }
    }

    fn is_list_type(&self, value: &LlValue) -> bool {
        matches!(value.ty, LlType::Ptr)
    }

    fn map_type(&self, data_type: &DataType) -> Result<LlType> {
        match data_type {
            DataType::I64 | DataType::Unknown => Ok(LlType::I64),
            DataType::I32 => Ok(LlType::I64),
            DataType::I8 | DataType::I16 => Ok(LlType::I64),
            DataType::U8 | DataType::U16 | DataType::U32 | DataType::U64 => Ok(LlType::I64),
            DataType::F32 | DataType::F64 => Ok(LlType::Ptr),
            DataType::Bool => Ok(LlType::I1),
            DataType::Str => Ok(LlType::Ptr),
            DataType::List
            | DataType::Vector { .. }
            | DataType::Dict
            | DataType::Map { .. }
            | DataType::Set
            | DataType::Tuple
            | DataType::Array { .. }
            | DataType::Slice { .. }
            | DataType::Struct => Ok(LlType::Ptr),
            DataType::None => Ok(LlType::I64),
            other => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Avenys does not yet lower type {:?}", other),
            })),
        }
    }

    fn runtime_kind_code(&self, data_type: &DataType) -> i64 {
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
            _ => 1,
        }
    }

    fn element_size(&self, data_type: &DataType) -> i64 {
        match data_type {
            DataType::Bool | DataType::I8 | DataType::U8 => 1,
            DataType::I16 | DataType::U16 => 2,
            DataType::I32 | DataType::U32 => 4,
            DataType::Str
            | DataType::List
            | DataType::Vector { .. }
            | DataType::Dict
            | DataType::Map { .. }
            | DataType::Set
            | DataType::Tuple
            | DataType::Array { .. }
            | DataType::Slice { .. }
            | DataType::F32
            | DataType::F64 => 8,
            _ => 8,
        }
    }

    fn scalar_storage_ir_type(&self, data_type: &DataType) -> &'static str {
        match data_type {
            DataType::Bool | DataType::I8 | DataType::U8 => "i8",
            DataType::I16 | DataType::U16 => "i16",
            DataType::I32 | DataType::U32 => "i32",
            _ => "i64",
        }
    }

    fn cast_scalar_for_store(
        &mut self,
        value: LlValue,
        data_type: &DataType,
    ) -> Result<(String, String)> {
        match data_type {
            DataType::Bool => {
                let bool_value = self.cast_to_i1(value)?;
                let widened = self.tmp();
                self.body
                    .push(format!("  {widened} = zext i1 {} to i8", bool_value.repr));
                Ok(("i8".to_string(), widened))
            }
            DataType::I8 | DataType::U8 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i8", scalar.repr));
                Ok(("i8".to_string(), narrowed))
            }
            DataType::I16 | DataType::U16 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i16", scalar.repr));
                Ok(("i16".to_string(), narrowed))
            }
            DataType::I32 | DataType::U32 => {
                let scalar = self.cast_to_i64(value)?;
                let narrowed = self.tmp();
                self.body
                    .push(format!("  {narrowed} = trunc i64 {} to i32", scalar.repr));
                Ok(("i32".to_string(), narrowed))
            }
            _ => {
                let scalar = self.cast_to_i64(value)?;
                Ok(("i64".to_string(), scalar.repr))
            }
        }
    }

    fn default_value(&mut self, ty: LlType) -> LlValue {
        match ty {
            LlType::I64 => LlValue {
                ty,
                repr: "0".to_string(),
                owned: false,
            },
            LlType::I1 => LlValue {
                ty,
                repr: "0".to_string(),
                owned: false,
            },
            LlType::Ptr => self.string_value(""),
            LlType::Struct(_) => LlValue {
                ty,
                repr: "null".to_string(),
                owned: false,
            },
        }
    }

    fn string_value(&mut self, value: &str) -> LlValue {
        let label = format!("@.str{}", self.strings.len());
        let escaped = escape_llvm_string(value);
        let len = string_byte_len(value) + 1;
        self.strings.push(format!(
            "{label} = private unnamed_addr constant [{len} x i8] c\"{escaped}\\00\""
        ));
        let tmp = self.tmp();
        self.body.push(format!(
            "  {tmp} = getelementptr inbounds [{len} x i8], ptr {label}, i64 0, i64 0"
        ));
        LlValue {
            ty: LlType::Ptr,
            repr: tmp,
            owned: false,
        }
    }

    fn cast_to_i64(&mut self, value: LlValue) -> Result<LlValue> {
        match value.ty {
            LlType::I64 => Ok(value),
            LlType::I1 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = zext i1 {} to i64", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Ptr | LlType::Struct(_) => Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot cast pointer/struct to i64".to_string(),
            })),
        }
    }

    fn cast_to_i1(&mut self, value: LlValue) -> Result<LlValue> {
        match value.ty {
            LlType::I1 => Ok(value),
            LlType::I64 => {
                let tmp = self.tmp();
                self.body
                    .push(format!("  {tmp} = icmp ne i64 {}, 0", value.repr));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: tmp,
                    owned: false,
                })
            }
            LlType::Ptr | LlType::Struct(_) => Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot cast pointer/struct to bool".to_string(),
            })),
        }
    }

    fn compile_binary(
        &mut self,
        op: &str,
        lhs: LlValue,
        rhs: LlValue,
        _data_type: &DataType,
    ) -> Result<LlValue> {
        let left_repr = lhs.repr.clone();
        let right_repr = rhs.repr.clone();
        let left_is_ptr = lhs.ty == LlType::Ptr;
        let right_is_ptr = rhs.ty == LlType::Ptr;
        let result = self.tmp();

        if left_is_ptr && right_is_ptr && op == "+" {
            self.body.push(format!(
                "  {result} = call ptr @mire_string_concat(ptr {left_repr}, ptr {right_repr})"
            ));
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: result,
                owned: true,
            });
        }

        if left_is_ptr && right_is_ptr && matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=") {
            let cmp_value = self.tmp();
            self.body.push(format!(
                "  {cmp_value} = call i32 @strcmp(ptr {left_repr}, ptr {right_repr})"
            ));
            let pred = match op {
                "==" => "eq",
                "!=" => "ne",
                "<" => "slt",
                ">" => "sgt",
                "<=" => "sle",
                ">=" => "sge",
                _ => unreachable!(),
            };
            self.body
                .push(format!("  {result} = icmp {pred} i32 {cmp_value}, 0"));
            return Ok(LlValue {
                ty: LlType::I1,
                repr: result,
                owned: false,
            });
        }

        match op {
            "+" => {
                self.body
                    .push(format!("  {result} = add i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "-" => {
                self.body
                    .push(format!("  {result} = sub i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "*" => {
                self.body
                    .push(format!("  {result} = mul i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "/" => {
                self.body
                    .push(format!("  {result} = udiv i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "%" => {
                self.body
                    .push(format!("  {result} = urem i64 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                let cmp = match op {
                    "==" => "eq",
                    "!=" => "ne",
                    "<" => "slt",
                    ">" => "sgt",
                    "<=" => "sle",
                    ">=" => "sge",
                    _ => "eq",
                };
                self.body.push(format!(
                    "  {result} = icmp {cmp} i64 {left_repr}, {right_repr}"
                ));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            "and" => {
                self.body
                    .push(format!("  {result} = and i1 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            "or" => {
                self.body
                    .push(format!("  {result} = or i1 {left_repr}, {right_repr}"));
                Ok(LlValue {
                    ty: LlType::I1,
                    repr: result,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Unknown operator: {}", op),
            })),
        }
    }

    fn compile_unary(&mut self, op: &str, value: LlValue) -> Result<LlValue> {
        let result = self.tmp();
        match op {
            "-" => {
                self.body
                    .push(format!("  {result} = sub i64 0, {}", value.repr));
                Ok(LlValue {
                    ty: LlType::I64,
                    repr: result,
                    owned: false,
                })
            }
            _ => Err(MireError::new(ErrorKind::Runtime {
                message: format!("Unknown unary operator: {}", op),
            })),
        }
    }

    fn compile_list_literal(
        &mut self,
        elements: &[Expression],
        element_type: &DataType,
    ) -> Result<LlValue> {
        let size = elements.len() as i64;
        if size == 0 {
            let ptr = self.tmp();
            self.body.push(format!("  {ptr} = inttoptr i64 0 to ptr"));
            return Ok(LlValue {
                ty: LlType::Ptr,
                repr: ptr,
                owned: false,
            });
        }
        let malloc = self.tmp();
        let list_ptr = self.tmp();
        let elem_size = self.element_size(element_type);
        self.body.push(format!(
            "  {malloc} = call i8* @malloc(i64 {})",
            16 + size * elem_size
        ));
        self.body
            .push(format!("  store i64 {}, ptr {malloc}", size));
        self.body.push(format!(
            "  {list_ptr} = getelementptr i8, ptr {malloc}, i64 8"
        ));
        self.body
            .push(format!("  store i64 {}, ptr {list_ptr}", size));
        let elem_ll_ty = self.map_type(element_type).unwrap_or(LlType::I64);
        for (i, elem) in elements.iter().enumerate() {
            let val = self.compile_expr(elem)?;
            let elem_ptr = self.tmp();
            self.body.push(format!(
                "  {elem_ptr} = getelementptr i8, ptr {}, i64 {}",
                list_ptr,
                8 + i as i64 * elem_size
            ));
            if elem_ll_ty == LlType::Ptr {
                let stored = self.cast_to_type(val, LlType::Ptr)?;
                self.body
                    .push(format!("  store ptr {}, ptr {}", stored.repr, elem_ptr));
            } else {
                let (store_ty, store_repr) = self.cast_scalar_for_store(val, element_type)?;
                self.body.push(format!(
                    "  store {} {}, ptr {}",
                    store_ty, store_repr, elem_ptr
                ));
            }
        }
        Ok(LlValue {
            ty: LlType::Ptr,
            repr: list_ptr,
            owned: false,
        })
    }

    fn concat_values(&mut self, lhs: LlValue, rhs: LlValue) -> LlValue {
        let result = self.tmp();
        self.body.push(format!(
            "  {result} = call ptr @mire_string_concat(ptr {}, ptr {})",
            lhs.repr, rhs.repr
        ));
        LlValue {
            ty: LlType::Ptr,
            repr: result,
            owned: true,
        }
    }

    fn compile_dict_literal(&mut self, entries: &[(Expression, Expression)]) -> Result<LlValue> {
        let mut current = LlValue {
            ty: LlType::Ptr,
            repr: "null".to_string(),
            owned: false,
        };

        for (key_expr, value_expr) in entries {
            let key_data_type = self.expression_data_type(key_expr);
            let value_data_type = self.expression_data_type(value_expr);
            let key = self.compile_expr(key_expr)?;
            let value = self.compile_expr(value_expr)?;
            let key_kind = self.runtime_kind_code(&key_data_type);
            let value_kind = self.runtime_kind_code(&value_data_type);
            let key_i64 = if key.ty == LlType::Ptr {
                LlValue {
                    ty: LlType::I64,
                    repr: "0".to_string(),
                    owned: false,
                }
            } else {
                self.cast_to_i64(key.clone())?
            };
            let key_ptr = if key.ty == LlType::Ptr {
                key
            } else {
                LlValue {
                    ty: LlType::Ptr,
                    repr: "null".to_string(),
                    owned: false,
                }
            };
            let result = self.tmp();

            if value.ty == LlType::Ptr {
                let casted = self.cast_to_type(value, LlType::Ptr)?;
                self.body.push(format!(
                    "  {result} = call ptr @mire_dict_set_ptr(ptr {}, i64 {}, i64 {}, i64 {}, ptr {}, ptr {})",
                    current.repr, key_kind, value_kind, key_i64.repr, key_ptr.repr, casted.repr
                ));
            } else {
                let casted = self.cast_to_i64(value)?;
                self.body.push(format!(
                    "  {result} = call ptr @mire_dict_set_i64(ptr {}, i64 {}, i64 {}, i64 {}, ptr {}, i64 {})",
                    current.repr, key_kind, value_kind, key_i64.repr, key_ptr.repr, casted.repr
                ));
            }

            current = LlValue {
                ty: LlType::Ptr,
                repr: result,
                owned: true,
            };
        }

        Ok(current)
    }

    fn cast_to_type(&mut self, value: LlValue, ty: LlType) -> Result<LlValue> {
        match ty {
            LlType::I64 => self.cast_to_i64(value),
            LlType::I1 => self.cast_to_i1(value),
            LlType::Ptr if value.ty == LlType::Ptr => Ok(value),
            LlType::Ptr => Err(MireError::new(ErrorKind::Runtime {
                message: "Avenys cannot cast non-pointer value to string".to_string(),
            })),
            LlType::Struct(_) => Ok(value),
        }
    }

    fn store_casted(&mut self, ptr: &str, ty: LlType, value: LlValue) -> Result<()> {
        let value = match ty {
            LlType::I64 => self.cast_to_i64(value)?,
            LlType::I1 => self.cast_to_i1(value)?,
            LlType::Ptr if value.ty == LlType::Ptr => value,
            LlType::Ptr => {
                return Err(MireError::new(ErrorKind::Runtime {
                    message: "Avenys cannot store non-pointer into string slot".to_string(),
                }))
            }
            LlType::Struct(_) => value,
        };
        self.body.push(format!(
            "  store {} {}, ptr {}",
            self.ty(ty),
            value.repr,
            ptr
        ));
        Ok(())
    }

    fn store_variable(
        &mut self,
        name: &str,
        ptr: &str,
        ty: LlType,
        data_type: DataType,
        value: LlValue,
    ) -> Result<()> {
        if data_type == DataType::Str && ty == LlType::Ptr {
            let old_owned = self
                .vars
                .get(name)
                .map(|var| var.owns_heap_string)
                .unwrap_or(false);

            if old_owned {
                let old_ptr = self.tmp();
                self.body.push(format!("  {old_ptr} = load ptr, ptr {ptr}"));
                self.body
                    .push(format!("  call void @mire_string_free(ptr {old_ptr})"));
            }

            let owned_value = if value.owned {
                value
            } else {
                let copied = self.tmp();
                self.body.push(format!(
                    "  {copied} = call ptr @mire_string_copy(ptr {})",
                    value.repr
                ));
                LlValue {
                    ty: LlType::Ptr,
                    repr: copied,
                    owned: true,
                }
            };

            self.store_casted(ptr, ty.clone(), owned_value)?;
            if let Some(var) = self.vars.get_mut(name) {
                var.data_type = data_type;
                var.owns_heap_string = true;
            }
            return Ok(());
        }

        self.store_casted(ptr, ty.clone(), value)?;
        if let Some(var) = self.vars.get_mut(name) {
            var.data_type = data_type;
            var.owns_heap_string = false;
        }
        Ok(())
    }

    fn try_compile_in_place_string_append(
        &mut self,
        target: &str,
        var: &VarInfo,
        value: &Expression,
    ) -> Result<bool> {
        if var.data_type != DataType::Str || var.ty != LlType::Ptr || !var.owns_heap_string {
            return Ok(false);
        }

        let Expression::BinaryOp {
            operator,
            left,
            right,
            ..
        } = value
        else {
            return Ok(false);
        };

        if operator != "+" {
            return Ok(false);
        }

        let Expression::Identifier(identifier) = left.as_ref() else {
            return Ok(false);
        };

        if identifier.name != target {
            return Ok(false);
        }

        let rhs = self.compile_expr(right)?;
        let current = self.tmp();
        let appended = self.tmp();
        self.body
            .push(format!("  {current} = load ptr, ptr {}", var.ptr));
        self.body.push(format!(
            "  {appended} = call ptr @mire_string_append_owned(ptr {current}, ptr {})",
            rhs.repr
        ));
        self.body
            .push(format!("  store ptr {appended}, ptr {}", var.ptr));
        if let Some(var) = self.vars.get_mut(target) {
            var.owns_heap_string = true;
        }
        Ok(true)
    }

    fn ty(&self, ty: LlType) -> &'static str {
        match ty {
            LlType::I64 => "i64",
            LlType::I1 => "i1",
            LlType::Ptr => "ptr",
            LlType::Struct(_) => "ptr",
        }
    }

    fn render_struct_ty(&self, fields: &[LlType]) -> String {
        let rendered = fields
            .iter()
            .map(|field| self.ty(field.clone()).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {} }}", rendered)
    }

    fn tmp(&mut self) -> String {
        let out = format!("%t{}", self.next_tmp);
        self.next_tmp += 1;
        out
    }

    fn label(&mut self, prefix: &str) -> String {
        let out = format!("{prefix}_{}", self.next_label);
        self.next_label += 1;
        out
    }

    fn compile_function_ir(
        &mut self,
        name: &str,
        params: &[(String, DataType)],
        body: &[Statement],
        ret: LlType,
    ) -> Result<String> {
        let saved_allocas = std::mem::take(&mut self.entry_allocas);
        let saved_body = std::mem::take(&mut self.body);
        let saved_vars = std::mem::take(&mut self.vars);
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_return = self.current_return.clone();
        self.current_return = ret.clone();

        let fn_info = self.user_functions.get(name).cloned().ok_or_else(|| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Avenys missing function metadata for '{}'", name),
            })
        })?;

        for ((param_name, _), param_ty) in params.iter().zip(fn_info.params.iter()) {
            let ptr = self.tmp();
            let arg_name = format!("%arg_{}", sanitize_symbol(param_name));
            let param_ty = param_ty.clone();
            self.entry_allocas
                .push(format!("  {ptr} = alloca {}", self.ty(param_ty.clone())));
            self.body.push(format!(
                "  store {} {}, ptr {}",
                self.ty(param_ty.clone()),
                arg_name,
                ptr
            ));
            self.vars.insert(
                param_name.clone(),
                VarInfo {
                    ptr,
                    ty: param_ty,
                    data_type: params
                        .iter()
                        .find(|(name, _)| name == param_name)
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(DataType::Unknown),
                    owns_heap_string: false,
                    struct_name: None,
                },
            );
        }

        for stmt in body {
            self.compile_statement(stmt)?;
        }

        let ret_clone = ret.clone();
        if body
            .iter()
            .all(|stmt| !matches!(stmt, Statement::Return(_)))
        {
            let default = self.default_value(ret_clone.clone());
            self.body.push(format!(
                "  ret {} {}",
                self.ty(ret_clone.clone()),
                default.repr
            ));
        }

        let args = params
            .iter()
            .zip(fn_info.params.iter())
            .map(|((name, _), ty)| {
                format!("{} %arg_{}", self.ty(ty.clone()), sanitize_symbol(name))
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut lines = Vec::new();
        lines.push(format!(
            "define {} {}({}) {{",
            self.ty(ret_clone.clone()),
            fn_info.llvm_name,
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

        Ok(lines.join("\n"))
    }
}

fn string_byte_len(value: &str) -> usize {
    value.as_bytes().len()
}

fn escape_llvm_string(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\r' => out.push_str("\\0D"),
            b'\t' => out.push_str("\\09"),
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{:02X}", byte)),
        }
    }
    out
}

fn sanitize_symbol(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}