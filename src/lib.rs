// Project manifests, lockfiles, and dependency metadata are compiler internals.
// Owl owns project orchestration and receives only the small compiler API
// re-exported below.
pub(crate) mod avens;
pub mod builtins;
pub mod compiler;
pub mod error;
pub mod incremental;
pub mod lexer;
pub mod loader;
pub mod parser;
pub mod types;

pub use avens::{
    BuildMode, BuildOptions, BuildResult, CDefs, EntryContainment, ImportMode, LibType,
    MireCacheConfig, MireManifest, MirePaths, MireProject, OptLevel, RuntimeTier,
    check_entry_containment, compile_file_with_avenys, default_output_dir, find_project_root,
    load_config_file, load_exports, load_project_manifest,
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
/// AST:   push::i64        ← kept by parser
///         ↓ canonical_fn_name
/// Typeck: push.i64         ← function lookup tables
///         ↓ canonical_fn_name
/// MIR:    push.i64         ← LLVM identifiers
#[inline]
pub fn canonical_fn_name(name: &str) -> String {
    name.replace("::", ".")
}

/// Namespace of the bit-level reinterpretation intrinsic.
pub const BITCAST_NS: &str = "bits";

/// Resolve `bits::<T>(x)` / `bits.<T>(x)` to the target type `T`.
///
/// Returns `None` when `name` is not a bitcast at all. `Some(DataType::Unknown)`
/// means it *is* a bitcast but the suffix is not a known type, so the caller
/// should report a diagnostic rather than fall through to normal call
/// resolution.
pub fn bitcast_target(name: &str) -> Option<crate::parser::ast::DataType> {
    let sep = if name.contains("::") { "::" } else { "." };
    let (ns, rest) = name.split_once(sep)?;
    if ns != BITCAST_NS || rest.is_empty() || rest.contains('.') || rest.contains(':') {
        return None;
    }
    Some(crate::parser::ast::DataType::parse_type(rest))
}
