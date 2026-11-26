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
pub mod protocol;
pub mod types;

pub use avens::{
    BuildMode, BuildOptions, BuildResult, BundleMode, CDefs, EntryContainment, ImportMode, LibType,
    MireCacheConfig, MireManifest, MirePaths, MireProject, OptLevel, RuntimeTier, SecurityConfig,
    SecurityMode, TrustTier, check_entry_containment, compile_file_with_avenys, default_output_dir,
    find_project_root, load_config_file, load_exports, load_project_manifest,
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
/// Returns `Some(target)` only when `T` names a real type, and `None` when
/// `name` is not a bitcast at all.
///
/// A suffix that does not parse as a type is deliberately *not* reported as a
/// malformed bitcast. `bits` is a namespace a program may define for itself,
/// and a call such as `bits::popcount(255)` has to reach ordinary call
/// resolution. Treating every `bits::<identifier>` as an intrinsic made any
/// package exporting a `bits` module unusable, with a diagnostic about bitcast
/// target types for a call that had nothing to do with bitcasts. When such a
/// suffix is genuinely a typo, the ordinary "unknown function" diagnostic names
/// the function that does not exist, which is the accurate complaint.
pub fn bitcast_target(name: &str) -> Option<crate::parser::ast::DataType> {
    let sep = if name.contains("::") { "::" } else { "." };
    let (ns, rest) = name.split_once(sep)?;
    if ns != BITCAST_NS || rest.is_empty() || rest.contains('.') || rest.contains(':') {
        return None;
    }
    match crate::parser::ast::DataType::parse_type(rest) {
        // `parse_type` reports an unrecognized name as `Unknown`. That is the
        // signal that this was never a type annotation.
        crate::parser::ast::DataType::Unknown => None,
        target => Some(target),
    }
}

#[cfg(test)]
mod bitcast_tests {
    use super::*;
    use crate::parser::ast::DataType;

    #[test]
    fn a_real_type_suffix_is_a_bitcast() {
        assert!(matches!(bitcast_target("bits::i64"), Some(DataType::I64)));
        assert!(matches!(bitcast_target("bits.f64"), Some(DataType::F64)));
        assert!(matches!(bitcast_target("bits::bool"), Some(DataType::Bool)));
    }

    #[test]
    fn a_non_type_suffix_is_not_a_bitcast() {
        // The regression: `bits` is a namespace a program may define for
        // itself. Claiming every `bits::<identifier>` as an intrinsic made any
        // package exporting a `bits` module unbuildable, reporting a bitcast
        // error for a call that had nothing to do with bitcasts.
        assert!(bitcast_target("bits::double").is_none());
        assert!(bitcast_target("bits::popcount").is_none());
        assert!(bitcast_target("bits::helper").is_none());
        assert!(bitcast_target("bits.double").is_none());
    }

    #[test]
    fn other_namespaces_are_never_bitcasts() {
        assert!(bitcast_target("vec::push").is_none());
        assert!(bitcast_target("str::concat").is_none());
        assert!(bitcast_target("dasu").is_none());
    }

    #[test]
    fn malformed_intrinsic_names_are_rejected() {
        assert!(bitcast_target("bits::").is_none());
        assert!(bitcast_target("bits::i64::extra").is_none());
    }
}
