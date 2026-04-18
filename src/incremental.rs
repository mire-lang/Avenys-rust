use crate::avens::{BuildMode, find_project_root};
use crate::error::{ErrorKind, MireError, Result};
use crate::parser::Program;
use crate::parser::ast::Statement;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const CACHE_DIR_NAME: &str = ".cache";
const CACHE_FILE_NAME: &str = "incremental.json";
const CACHE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImport {
    pub raw_path: String,
    pub resolved_path: PathBuf,
    pub items: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedParsedFile {
    pub hash: u64,
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
    pub emit_binary: bool,
    pub persist_ir: bool,
    pub binary_path: PathBuf,
    pub ir_path: Option<PathBuf>,
    pub optimized_ir_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CacheDb {
    format_version: u32,
    files: HashMap<String, CachedParsedFile>,
    builds: HashMap<String, BuildCacheEntry>,
}

pub struct IncrementalCache {
    cache_path: PathBuf,
    db: CacheDb,
}

impl IncrementalCache {
    pub fn load_for(source_path: &Path) -> Self {
        let cache_path = cache_file_path(source_path);
        let db = fs::read_to_string(&cache_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<CacheDb>(&raw).ok())
            .filter(|db| db.format_version == CACHE_FORMAT_VERSION)
            .unwrap_or_else(|| CacheDb {
                format_version: CACHE_FORMAT_VERSION,
                ..CacheDb::default()
            });

        Self { cache_path, db }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Could not create incremental cache directory '{}': {}",
                        parent.display(),
                        err
                    ),
                })
            })?;
        }

        let raw = serde_json::to_string_pretty(&self.db).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not serialize incremental cache: {}", err),
            })
        })?;
        fs::write(&self.cache_path, raw).map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Could not write incremental cache '{}': {}",
                    self.cache_path.display(),
                    err
                ),
            })
        })
    }

    pub fn cached_file(&self, path: &Path, hash: u64) -> Option<CachedParsedFile> {
        self.db
            .files
            .get(&normalize_path_key(path))
            .filter(|entry| entry.hash == hash)
            .cloned()
    }

    pub fn store_file(&mut self, path: &Path, entry: CachedParsedFile) {
        self.db.files.insert(normalize_path_key(path), entry);
    }

    pub fn build_entry(
        &self,
        source_path: &Path,
        mode: BuildMode,
        emit_binary: bool,
        persist_ir: bool,
    ) -> Option<&BuildCacheEntry> {
        self.db
            .builds
            .get(&build_cache_key(source_path, mode, emit_binary, persist_ir))
    }

    pub fn store_build(&mut self, source_path: &Path, entry: BuildCacheEntry) {
        self.db.builds.insert(
            build_cache_key(source_path, entry.mode, entry.emit_binary, entry.persist_ir),
            entry,
        );
    }
}

pub fn cache_file_path(source_path: &Path) -> PathBuf {
    let base = if let Some(project_root) =
        find_project_root(source_path.parent().unwrap_or_else(|| Path::new(".")))
    {
        project_root.join("bin")
    } else {
        source_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    };

    base.join(CACHE_DIR_NAME).join(CACHE_FILE_NAME)
}

pub fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

pub fn build_fingerprint(
    source_path: &Path,
    files: &HashMap<PathBuf, LoadedFile>,
    mode: BuildMode,
    emit_binary: bool,
    runtime_support: &str,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    normalize_path_key(source_path).hash(&mut hasher);
    mode.hash(&mut hasher);
    emit_binary.hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    runtime_support.hash(&mut hasher);

    let mut file_entries: Vec<_> = files.iter().collect();
    file_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (path, info) in file_entries {
        normalize_path_key(path).hash(&mut hasher);
        info.hash.hash(&mut hasher);

        let mut deps = info.direct_dependencies.clone();
        deps.sort();
        for dependency in deps {
            normalize_path_key(&dependency).hash(&mut hasher);
        }
    }

    hasher.finish()
}

fn build_cache_key(
    source_path: &Path,
    mode: BuildMode,
    emit_binary: bool,
    persist_ir: bool,
) -> String {
    format!(
        "{}::{mode:?}::{emit_binary}::{persist_ir}",
        normalize_path_key(source_path)
    )
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn statement_export_name(statement: &Statement) -> Option<&str> {
    match statement {
        Statement::Let { name, .. }
        | Statement::Function { name, .. }
        | Statement::Type { name, .. }
        | Statement::Class { name, .. }
        | Statement::Trait { name, .. }
        | Statement::Skill { name, .. }
        | Statement::Module { name, .. }
        | Statement::Enum { name, .. }
        | Statement::ExternLib { name, .. }
        | Statement::ExternFunction { name, .. } => Some(name.as_str()),
        _ => None,
    }
}