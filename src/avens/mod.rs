use crate::compiler::{
    AnalysisSelection, analyze_program_with_origins, analyze_program_with_origins_partial,
};
use crate::error::diagnostic::Severity;
use crate::error::format::format_diagnostic;
use crate::error::{ErrorKind, MireError, Result};
use crate::incremental::{
    BuildCacheEntry, CacheSettings, CachedAnalysis, IncrementalCache, build_fingerprint,
    source_hash,
};

use crate::parser::ast::{
    AssignmentTarget, DataType, Expression, Identifier, Literal, Program, Statement,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod build_pipeline;
mod config;
mod llvm_binary;
mod llvm_builtins;
mod llvm_collections;
mod llvm_control;
mod llvm_dicts;
mod llvm_functions;
mod llvm_helpers;
mod llvm_lists;
mod llvm_strings;
mod llvm_types;
mod manifest;
mod reuse;
mod toolchain;
mod utils;
pub use build_pipeline::{compile_file_with_avenys, default_output_dir};
pub use config::{
    BootstrapConfig, BuildMode, BuildOptions, BuildResult, ExportsSection, ImportMode,
    MireCacheConfig, MireDependencies, MireDependency, MireLock, MireLockBuild, MireLockProject,
    MireManifest, MireProject, OptLevel,
};
use llvm_types::*;
pub use manifest::{
    find_project_root, load_exports, load_manifest_dependencies, load_project_manifest,
    project_lock_path, project_manifest_path, resolve_export_path, write_lock_file, write_manifest,
};
use reuse::prepare_program_with_partial_analysis_reuse;
use toolchain::{compile_binary_from_ir, optimize_ir};
use utils::{
    escape_llvm_string, normalize_nominal_name, sanitize_symbol, string_byte_len,
    strip_root_namespace,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct LlvmIrGen {
    strings: Vec<String>,
    functions: Vec<String>,
    entry_allocas: Vec<String>,
    body: Vec<String>,
    vars: HashMap<String, VarInfo>,
    function_aliases: HashMap<String, String>,
    function_value_signatures: HashMap<String, FnInfo>,
    user_functions: HashMap<String, FnInfo>,
    user_structs: HashMap<String, StructInfo>,
    user_enums: HashMap<String, EnumInfo>,
    extern_decls: Vec<String>,
    extern_libs: Vec<(String, String)>,
    loop_stack: Vec<LoopLabels>,
    current_return: LlType,
    current_function: String,
    next_fn_id: HashMap<String, usize>,
    current_line: usize,
    current_column: usize,
    next_tmp: usize,
    next_label: usize,
    emitted_monomorph_wrappers: HashSet<String>,
    owned_temps: HashSet<String>,
}

impl LlvmIrGen {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            functions: Vec::new(),
            entry_allocas: Vec::new(),
            body: Vec::new(),
            vars: HashMap::new(),
            function_aliases: HashMap::new(),
            function_value_signatures: HashMap::new(),
            user_functions: HashMap::new(),
            user_structs: HashMap::new(),
            user_enums: HashMap::new(),
            extern_decls: Vec::new(),
            extern_libs: Vec::new(),
            loop_stack: Vec::new(),
            current_return: LlType::I64,
            current_function: String::new(),
            next_fn_id: HashMap::new(),
            current_line: 1,
            current_column: 1,
            next_tmp: 0,
            next_label: 0,
            emitted_monomorph_wrappers: HashSet::new(),
            owned_temps: HashSet::new(),
        }
    }
}
