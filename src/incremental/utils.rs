use super::*;
use std::fs;

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
    base.join(CACHE_DIR_NAME)
}

pub fn source_hash(source: &str) -> u64 {
    let mut hasher = FxHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

pub fn source_hash2(source: &str) -> u64 {
    let mut hasher = FxHasher::with_seed(0x9e3779b97f4a7c15);
    source.hash(&mut hasher);
    hasher.finish()
}

pub fn build_fingerprint(
    source_path: &Path,
    files: &HashMap<PathBuf, LoadedFile>,
    mode: BuildMode,
    import_mode: ImportMode,
    opt_level: OptLevel,
    emit_binary: bool,
    c_sources_combined: &str,
) -> u64 {
    let mut hasher = FxHasher::new();
    normalize_path_key(source_path).hash(&mut hasher);
    mode.hash(&mut hasher);
    import_mode.hash(&mut hasher);
    opt_level.hash(&mut hasher);
    emit_binary.hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    c_sources_combined.hash(&mut hasher);

    let mut file_entries: Vec<_> = files.iter().collect();
    file_entries.sort_by_key(|(left, _)| *left);
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

pub(crate) fn build_cache_key(
    source_path: &Path,
    mode: BuildMode,
    import_mode: ImportMode,
    emit_binary: bool,
    persist_ir: bool,
) -> String {
    format!(
        "{}::{mode:?}::{import_mode:?}::{emit_binary}::{persist_ir}",
        normalize_path_key(source_path)
    )
}

pub(crate) fn mir_cache_key(
    source_path: &Path,
    fn_name: &str,
    body_hash: u64,
    opt_level: OptLevel,
) -> String {
    format!(
        "{}::mir::{}::{:#x}::opt={opt_level:?}",
        normalize_path_key(source_path),
        fn_name,
        body_hash,
    )
}

pub(crate) fn analysis_cache_key(source_path: &Path, source_hash: u64) -> String {
    format!(
        "{}::analysis::{:#x}",
        normalize_path_key(source_path),
        source_hash
    )
}

pub(crate) fn normalize_path_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

pub fn statement_export_name(statement: &Statement) -> Option<&str> {
    match statement {
        Statement::Let { name, .. }
        | Statement::Function { name, .. }
        | Statement::Type { name, .. }
        | Statement::Skill { name, .. }
        | Statement::Module { name, .. }
        | Statement::Enum { name, .. }
        | Statement::ExternLib { name, .. } => Some(name.as_str()),
        Statement::ExternFunction { name, visibility, .. } => {
            if *visibility == Visibility::Public {
                Some(name.as_str())
            } else {
                None
            }
        }
        Statement::Load { path, alias, .. } => Some(
            alias.as_deref().unwrap_or_else(|| path.last().map(|s| s.as_str()).unwrap_or("")),
        ),
        _ => None,
    }
}

    pub(crate) fn manifest_cache_settings(source_path: &Path) -> Result<CacheSettings> {
    let Some(project_root) =
        find_project_root(source_path.parent().unwrap_or_else(|| Path::new(".")))
    else {
        return Ok(CacheSettings::defaults());
    };

    let manifest_path = project_root.join("owl.toml");
    let raw = match fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(_) => return Ok(CacheSettings::defaults()),
    };

    #[derive(Deserialize)]
    struct ManifestFile {
        cache: Option<ManifestCache>,
    }

    #[derive(Deserialize)]
    struct ManifestCache {
        max_units: Option<usize>,
        analysis_cache: Option<bool>,
        compression: Option<bool>,
    }

    let manifest = toml::from_str::<ManifestFile>(&raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Invalid owl.toml cache configuration: {}", err),
        })
    })?;
    let defaults = CacheSettings::defaults();
    let cache = manifest.cache;
    let max_units = cache
        .as_ref()
        .and_then(|cache| cache.max_units)
        .unwrap_or(DEFAULT_MAX_UNITS);

    Ok(CacheSettings {
        max_units: (max_units != 0).then_some(max_units),
        analysis_cache: cache
            .as_ref()
            .and_then(|cache| cache.analysis_cache)
            .unwrap_or(defaults.analysis_cache),
        compression: cache
            .as_ref()
            .and_then(|cache| cache.compression)
            .unwrap_or(defaults.compression),
    })
}
