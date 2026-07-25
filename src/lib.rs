pub mod avens;
pub mod builtins;
pub mod compiler;
pub mod error;
pub mod incremental;
pub mod lexer;
pub mod loader;
pub mod parser;
pub mod types;

pub use avens::{
    BuildMode, BuildOptions, BuildResult, ImportMode, MireCacheConfig, MireDependencies,
    MireDependency, MireLock, MireManifest, MireProject, OptLevel, compile_file_with_avenys,
    default_output_dir, find_project_root, load_exports, load_manifest_dependencies,
    load_project_manifest, project_lock_path, project_manifest_path, write_lock_file,
    write_manifest,
};
pub use compiler::{
    AnalysisReport, WarningConfig, analyze_program, analyze_program_with_warnings,
    analyze_program_with_warnings_and_origins, check_program_types,
};
pub use error::mss::MssError;
pub use error::{ErrorKind, MireError, Result};
pub use incremental::{CacheOverrides, CacheSettings, LoadedProgram, cache_file_path};
pub use lexer::{Token, TokenType, tokenize};
pub use loader::{
    load_program_from_file, load_program_with_metadata, load_program_with_metadata_with_settings,
};
pub use parser::parse;
pub use parser::{MireValue, Program};

/// Normalize `::` separators in function names to `.`.
///
/// The parser preserves `::` in fn declaration names (e.g., `push::i64`) so the
/// renamer can distinguish original names from already-prefixed ones. Downstream
/// compiler passes that need `.`-separated identifiers call this function exactly
/// once at their boundary.
///
/// ```text
/// AST:   push::i64        ← kept by parser
///         ↓ canonical_fn_name
/// Typeck: push.i64         ← function lookup tables
///         ↓ canonical_fn_name
/// MIR:    push.i64         ← LLVM identifiers
/// ```
#[inline]
pub fn canonical_fn_name(name: &str) -> String {
    name.replace("::", ".")
}
