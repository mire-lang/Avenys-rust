use crate::compiler::{
    AnalysisSelection, analyze_program_with_origins, analyze_program_with_origins_partial,
};
use crate::error::diagnostic::Severity;
use crate::error::format::format_diagnostic;
use crate::error::{ErrorKind, MireError, Result};
use crate::incremental::{
    BuildCacheEntry, CacheSettings, CachedAnalysis, IncrementalCache, build_fingerprint,
    dependency_fingerprint, source_hash,
};
use crate::parser::ast::{Program, Statement};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod build_pipeline;
mod build_support;
mod config;
mod manifest;
mod reuse;
mod toolchain;
pub use build_pipeline::{compile_file_with_avenys, default_output_dir};
pub use config::{
    BootstrapConfig, BuildMode, BuildOptions, BuildResult, ExportsSection, ImportMode,
    MireCacheConfig, MireDependencies, MireDependency, MireLock, MireLockBuild, MireLockProject,
    MireManifest, MireProject, OptLevel,
};
pub use manifest::{
    find_project_root, load_exports, load_manifest_dependencies, load_project_manifest,
    project_lock_path, project_manifest_path, resolve_export_path, write_lock_file, write_manifest,
};
use reuse::prepare_program_with_partial_analysis_reuse;
use toolchain::{compile_binary_from_ir, optimize_ir};
