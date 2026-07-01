use crate::avens::{BuildMode, ImportMode, OptLevel, find_project_root};
use crate::error::mss::MssError;
use crate::error::{ErrorKind, MireError, Result};
use crate::parser::Program;
use crate::parser::ast::{
    AssignmentTarget, DataType, EnumVariantDef, Expression, Identifier, Literal, QueryBinding,
    QueryGroup, QueryJoin, QueryOp, Statement, TraitMethodSig, Visibility,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

mod analysis;
mod dependencies;
mod hasher;
mod hashing;

pub use analysis::{
    analysis_child_unit_key, analysis_unit_key, analysis_units_for_program,
    compute_invalidation_report,
};
pub(crate) use dependencies::collect_statement_bindings;
pub(crate) use dependencies::collect_statement_dependencies;
pub use hasher::FxHasher;

mod cache;
mod lru;
mod utils;
pub(crate) use utils::{
    analysis_cache_key, build_cache_key, manifest_cache_settings, mir_cache_key, normalize_path_key,
};
pub use utils::{
    build_fingerprint, cache_file_path, source_hash, source_hash2, statement_export_name,
};

#[cfg(test)]
pub(crate) fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

const CACHE_DIR_NAME: &str = ".cache";
const DEFAULT_MAX_UNITS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheSettings {
    pub max_units: Option<usize>,
    pub analysis_cache: bool,
    pub compression: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CacheOverrides {
    pub max_units: Option<usize>,
    pub analysis_cache: Option<bool>,
    pub compression: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImport {
    pub raw_path: String,
    pub resolved_path: PathBuf,
    pub items: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CachedParsedFile {
    pub hash: u64,
    pub hash2: u64,
    pub program: Program,
    pub exports: Vec<String>,
    pub local_imports: Vec<CachedImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredParsedFile {
    pub program: Program,
    pub exports: Vec<String>,
    pub local_imports: Vec<CachedImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedFile {
    pub hash: u64,
    pub direct_dependencies: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoadedProgram {
    pub program: Program,
    pub files: HashMap<PathBuf, LoadedFile>,
    pub statement_origins: Vec<PathBuf>,
    pub sources: HashMap<PathBuf, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCacheEntry {
    pub fingerprint: u64,
    pub mode: BuildMode,
    pub import_mode: ImportMode,
    pub opt_level: OptLevel,
    pub emit_binary: bool,
    pub persist_ir: bool,
    pub binary_path: PathBuf,
    pub ir_path: Option<PathBuf>,
    pub optimized_ir_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAnalyzedProgram {
    pub program: Program,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAnalysisPayload {
    pub outcome: StoredAnalysisOutcome,
    pub units: Vec<AnalysisUnitMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum StoredAnalysisOutcome {
    Success(StoredAnalyzedProgram),
    Error(StoredMireError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisUnitMetadata {
    pub unit_key: String,
    pub unit_kind: AnalysisUnitKind,
    pub body_hash: u64,
    pub body_hash2: u64,
    pub dependencies: Vec<String>,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisUnitKind {
    Function,
    Type,
    Enum,
    Impl,
    Field,
    Other,
}

#[derive(Debug, Clone)]
pub enum CachedAnalysis {
    Success(Program),
    Error(MireError),
}

#[derive(Debug, Clone)]
pub struct CachedAnalysisSnapshot {
    pub program: Program,
    pub units: Vec<AnalysisUnitMetadata>,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisInvalidationReport {
    pub changed_units: Vec<String>,
    pub invalidated_units: Vec<String>,
    pub added_units: Vec<String>,
    pub removed_units: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub file_hits: u64,
    pub file_misses: u64,
    pub analysis_hits: u64,
    pub analysis_misses: u64,
    pub build_hits: u64,
    pub build_misses: u64,
    pub evictions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredMireError {
    pub kind: StoredErrorKind,
    pub source: Option<String>,
    pub filename: Option<String>,
    pub line: usize,
    pub column: usize,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum StoredErrorKind {
    Lexer {
        line: usize,
        column: usize,
        message: String,
    },
    DeprecatedSyntax {
        line: usize,
        column: usize,
        message: String,
    },
    Parser {
        line: usize,
        column: usize,
        message: String,
    },
    Backend {
        message: String,
    },
    Runtime {
        message: String,
    },
    Type {
        line: usize,
        column: usize,
        message: String,
    },
    Ownership {
        line: usize,
        column: usize,
        kind: StoredMssError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum StoredMssError {
    MutationWhileShared,
    MultipleMutableRefs,
    MoveWhileBorrowed,
    UseAfterMove,
    DropWhileBorrowed,
    DoubleDrop,
    BorrowOutOfScope,
    InvalidMove,
    UnsafeViolation,
}

// Re-export the new IncrementalCache
pub use cache::IncrementalCache;

// The old serialize.rs types are still needed for backward compat reading old cache files
// (StoredMireError conversions, etc.)
impl From<&MireError> for StoredMireError {
    fn from(value: &MireError) -> Self {
        Self {
            kind: (&value.kind).into(),
            source: value.source().cloned(),
            filename: value.filename().cloned(),
            line: value.line,
            column: value.column,
            explanation: value.explanation().cloned(),
        }
    }
}

impl From<StoredMireError> for MireError {
    fn from(value: StoredMireError) -> Self {
        let mut error = MireError::new(value.kind.into());
        error.set_source(value.source);
        error.set_filename(value.filename);
        error.line = value.line;
        error.column = value.column;
        error.set_explanation(value.explanation);
        error
    }
}

impl From<&ErrorKind> for StoredErrorKind {
    fn from(value: &ErrorKind) -> Self {
        match value {
            ErrorKind::Lexer {
                line,
                column,
                message,
            } => Self::Lexer {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            ErrorKind::DeprecatedSyntax {
                line,
                column,
                message,
            } => Self::DeprecatedSyntax {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            ErrorKind::Parser {
                line,
                column,
                message,
            } => Self::Parser {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            ErrorKind::Backend { message } => Self::Backend {
                message: message.clone(),
            },
            ErrorKind::Runtime { message } => Self::Runtime {
                message: message.clone(),
            },
            ErrorKind::Type {
                line,
                column,
                message,
            } => Self::Type {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            ErrorKind::Ownership { line, column, kind } => Self::Ownership {
                line: *line,
                column: *column,
                kind: kind.into(),
            },
        }
    }
}

impl From<StoredErrorKind> for ErrorKind {
    fn from(value: StoredErrorKind) -> Self {
        match value {
            StoredErrorKind::Lexer {
                line,
                column,
                message,
            } => Self::Lexer {
                line,
                column,
                message,
            },
            StoredErrorKind::DeprecatedSyntax {
                line,
                column,
                message,
            } => Self::DeprecatedSyntax {
                line,
                column,
                message,
            },
            StoredErrorKind::Parser {
                line,
                column,
                message,
            } => Self::Parser {
                line,
                column,
                message,
            },
            StoredErrorKind::Backend { message } => Self::Backend { message },
            StoredErrorKind::Runtime { message } => Self::Runtime { message },
            StoredErrorKind::Type {
                line,
                column,
                message,
            } => Self::Type {
                line,
                column,
                message,
            },
            StoredErrorKind::Ownership { line, column, kind } => Self::Ownership {
                line,
                column,
                kind: kind.into(),
            },
        }
    }
}

impl From<&MssError> for StoredMssError {
    fn from(value: &MssError) -> Self {
        match value {
            MssError::MutationWhileShared => Self::MutationWhileShared,
            MssError::MultipleMutableRefs => Self::MultipleMutableRefs,
            MssError::MoveWhileBorrowed => Self::MoveWhileBorrowed,
            MssError::UseAfterMove => Self::UseAfterMove,
            MssError::DropWhileBorrowed => Self::DropWhileBorrowed,
            MssError::DoubleDrop => Self::DoubleDrop,
            MssError::BorrowOutOfScope => Self::BorrowOutOfScope,
            MssError::InvalidMove => Self::InvalidMove,
            MssError::UnsafeViolation => Self::UnsafeViolation,
        }
    }
}

impl From<StoredMssError> for MssError {
    fn from(value: StoredMssError) -> Self {
        match value {
            StoredMssError::MutationWhileShared => Self::MutationWhileShared,
            StoredMssError::MultipleMutableRefs => Self::MultipleMutableRefs,
            StoredMssError::MoveWhileBorrowed => Self::MoveWhileBorrowed,
            StoredMssError::UseAfterMove => Self::UseAfterMove,
            StoredMssError::DropWhileBorrowed => Self::DropWhileBorrowed,
            StoredMssError::DoubleDrop => Self::DoubleDrop,
            StoredMssError::BorrowOutOfScope => Self::BorrowOutOfScope,
            StoredMssError::InvalidMove => Self::InvalidMove,
            StoredMssError::UnsafeViolation => Self::UnsafeViolation,
        }
    }
}

// CacheSettings methods
impl CacheSettings {
    pub fn defaults() -> Self {
        Self {
            max_units: Some(DEFAULT_MAX_UNITS),
            analysis_cache: true,
            compression: false,
        }
    }

    pub fn resolve_for(source_path: &Path, overrides: CacheOverrides) -> Result<Self> {
        let mut resolved = manifest_cache_settings(source_path)?;
        if let Some(max_units) = overrides.max_units {
            resolved.max_units = (max_units != 0).then_some(max_units);
        }
        if let Some(enabled) = overrides.analysis_cache {
            resolved.analysis_cache = enabled;
        }
        if let Some(enabled) = overrides.compression {
            resolved.compression = enabled;
        }
        Ok(resolved)
    }
}
