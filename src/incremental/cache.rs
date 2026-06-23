use super::lru::LruMap;
use super::*;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION_FILE: &str = "version.txt";
const INDEX_DIR: &str = "index";
const BLOBS_DIR: &str = "blobs";
const WAL_DIR: &str = "wal";
const NEW_CACHE_FORMAT: &str = "MIREINC4";
const NEW_FORMAT_VERSION: u32 = 1;
const FILES_INDEX: &str = "files";
const ANALYSES_INDEX: &str = "analyses";
const BUILDS_INDEX: &str = "builds";
const MIR_INDEX: &str = "mir";
// ── WAL records ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
enum WalRecord {
    StoreFile {
        key: String,
        hash: u64,
        hash2: u64,
        blob_hash: String,
        timestamp: u64,
    },
    StoreAnalysis {
        key: String,
        fingerprint: u64,
        blob_hash: String,
        timestamp: u64,
        created_ms: u64,
        unit_count: u32,
    },
    StoreBuild {
        key: String,
        entry: BuildCacheEntry,
        timestamp: u64,
    },
    DeleteFile {
        key: String,
        timestamp: u64,
    },
    DeleteAnalysis {
        key: String,
        timestamp: u64,
    },
    DeleteBuild {
        key: String,
        timestamp: u64,
    },
    StoreMirFn {
        key: String,
        body_hash: u64,
        blob_hash: String,
        timestamp: u64,
    },
    DeleteMirFn {
        key: String,
        timestamp: u64,
    },
    Checkpoint {
        timestamp: u64,
    },
}

// ── Meta files (per-entry index) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileMeta {
    hash: u64,
    hash2: u64,
    blob_hash: String,
    last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnalysisMeta {
    fingerprint: u64,
    blob_hash: String,
    last_access_ms: u64,
    created_ms: u64,
    unit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildMeta {
    fingerprint: u64,
    entry: BuildCacheEntry,
    last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MirMeta {
    body_hash: u64,
    blob_hash: String,
    last_access_ms: u64,
}

// ── WAL helpers ─────────────────────────────────────────────────────────

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn write_wal(base_dir: &Path, records: &[WalRecord]) -> Result<()> {
    let wal_dir = base_dir.join(WAL_DIR);
    fs::create_dir_all(&wal_dir).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot create WAL dir: {e}"),
        })
    })?;
    let path = wal_dir.join(format!("{}.wal", timestamp_ms()));
    let mut file = fs::File::create(&path).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot create WAL file: {e}"),
        })
    })?;
    for rec in records {
        let line = serde_json::to_string(rec).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot serialize WAL record: {e}"),
            })
        })?;
        writeln!(file, "{line}").map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot write WAL record: {e}"),
            })
        })?;
    }
    file.sync_all().ok();
    Ok(())
}

fn replay_wal(base_dir: &Path) -> Result<Vec<WalRecord>> {
    let wal_dir = base_dir.join(WAL_DIR);
    if !wal_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = fs::read_dir(&wal_dir)
        .map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot read WAL dir: {e}"),
            })
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "wal"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut all_records = Vec::new();
    for entry in entries {
        let content = fs::read_to_string(entry.path()).unwrap_or_default();
        for line in content.lines() {
            if let Ok(rec) = serde_json::from_str::<WalRecord>(line) {
                all_records.push(rec);
            }
        }
    }
    Ok(all_records)
}

fn clear_wal(base_dir: &Path) -> Result<()> {
    let wal_dir = base_dir.join(WAL_DIR);
    if wal_dir.exists() {
        for e in fs::read_dir(&wal_dir).ok().into_iter().flatten().flatten() {
            let _ = fs::remove_file(e.path());
        }
    }
    Ok(())
}

// ── Blob helpers ────────────────────────────────────────────────────────

fn compute_blob_hash(blob: &[u8]) -> String {
    use std::hash::Hasher;
    let mut hasher = FxHasher::new();
    hasher.write(blob);
    format!("{:016x}", hasher.finish())
}

fn store_blob(base_dir: &Path, blob: &[u8]) -> Result<String> {
    let hash = compute_blob_hash(blob);
    let blob_dir = base_dir.join(BLOBS_DIR);
    fs::create_dir_all(&blob_dir).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot create blobs dir: {e}"),
        })
    })?;
    let path = blob_dir.join(&hash);
    if !path.exists() {
        fs::write(&path, blob).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot write blob: {e}"),
            })
        })?;
    }
    Ok(hash)
}

fn read_blob(base_dir: &Path, blob_hash: &str) -> Option<Vec<u8>> {
    let path = base_dir.join(BLOBS_DIR).join(blob_hash);
    fs::read(&path).ok()
}

fn gc_blobs(base_dir: &Path, referenced: &HashSet<String>) -> Result<()> {
    let blob_dir = base_dir.join(BLOBS_DIR);
    if !blob_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&blob_dir).ok().into_iter().flatten() {
        if let Ok(e) = entry
            && let Some(name) = e.file_name().to_str()
                && !referenced.contains(name) {
                    let _ = fs::remove_file(e.path());
                }
    }
    Ok(())
}

// ── Index helpers ───────────────────────────────────────────────────────

fn key_hash(key: &str) -> String {
    use std::hash::Hasher;
    let mut hasher = FxHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn file_meta_path(base_dir: &Path, key: &str) -> PathBuf {
    base_dir
        .join(INDEX_DIR)
        .join(FILES_INDEX)
        .join(format!("{}.meta", key_hash(key)))
}

fn analysis_meta_path(base_dir: &Path, key: &str) -> PathBuf {
    base_dir
        .join(INDEX_DIR)
        .join(ANALYSES_INDEX)
        .join(format!("{}.meta", key_hash(key)))
}

fn build_meta_path(base_dir: &Path, key: &str) -> PathBuf {
    base_dir
        .join(INDEX_DIR)
        .join(BUILDS_INDEX)
        .join(format!("{}.meta", key_hash(key)))
}

fn mir_meta_path(base_dir: &Path, key: &str) -> PathBuf {
    base_dir
        .join(INDEX_DIR)
        .join(MIR_INDEX)
        .join(format!("{}.meta", key_hash(key)))
}

fn write_file_meta(base_dir: &Path, key: &str, meta: &FileMeta) -> Result<()> {
    let path = file_meta_path(base_dir, key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string(meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot serialize file meta: {e}"),
        })
    })?;
    fs::write(&path, &json).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot write file meta: {e}"),
        })
    })
}

fn read_file_meta(base_dir: &Path, key: &str) -> Option<FileMeta> {
    let path = file_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_analysis_meta(base_dir: &Path, key: &str, meta: &AnalysisMeta) -> Result<()> {
    let path = analysis_meta_path(base_dir, key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string(meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot serialize analysis meta: {e}"),
        })
    })?;
    fs::write(&path, &json).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot write analysis meta: {e}"),
        })
    })
}

fn read_analysis_meta(base_dir: &Path, key: &str) -> Option<AnalysisMeta> {
    let path = analysis_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_build_meta(base_dir: &Path, key: &str, meta: &BuildMeta) -> Result<()> {
    let path = build_meta_path(base_dir, key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string(meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot serialize build meta: {e}"),
        })
    })?;
    fs::write(&path, &json).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot write build meta: {e}"),
        })
    })
}

fn write_mir_meta(base_dir: &Path, key: &str, meta: &MirMeta) -> Result<()> {
    let path = mir_meta_path(base_dir, key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string(meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot serialize mir meta: {e}"),
        })
    })?;
    fs::write(&path, &json).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Cannot write mir meta: {e}"),
        })
    })
}

fn read_mir_meta(base_dir: &Path, key: &str) -> Option<MirMeta> {
    let path = mir_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_build_meta(base_dir: &Path, key: &str) -> Option<BuildMeta> {
    let path = build_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn collect_referenced_blobs(base_dir: &Path) -> Result<HashSet<String>> {
    let mut referenced = HashSet::new();

    // scan file metas
    let files_dir = base_dir.join(INDEX_DIR).join(FILES_INDEX);
    if let Ok(entries) = fs::read_dir(&files_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(meta) = serde_json::from_str::<FileMeta>(&content) {
                    referenced.insert(meta.blob_hash);
                }
        }
    }

    // scan analysis metas
    let analyses_dir = base_dir.join(INDEX_DIR).join(ANALYSES_INDEX);
    if let Ok(entries) = fs::read_dir(&analyses_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(meta) = serde_json::from_str::<AnalysisMeta>(&content) {
                    referenced.insert(meta.blob_hash);
                }
        }
    }

    // Build metas have no blob references (stored inline)

    // scan mir metas
    let mir_dir = base_dir.join(INDEX_DIR).join(MIR_INDEX);
    if let Ok(entries) = fs::read_dir(&mir_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(meta) = serde_json::from_str::<MirMeta>(&content) {
                    referenced.insert(meta.blob_hash);
                }
        }
    }

    // scan WAL for pending blob hashes
    if let Ok(records) = replay_wal(base_dir) {
        for rec in &records {
            match rec {
                WalRecord::StoreFile { blob_hash, .. }
                | WalRecord::StoreAnalysis { blob_hash, .. }
                | WalRecord::StoreMirFn { blob_hash, .. } => {
                    referenced.insert(blob_hash.clone());
                }
                _ => {}
            }
        }
    }

    Ok(referenced)
}

// ── New IncrementalCache ─────────────────────────────────────────────────

pub struct IncrementalCache {
    cache_dir: PathBuf,
    settings: CacheSettings,
    // In-memory state (hot cache). Cold entries live as meta files on disk.
    files: HashMap<String, FileMeta>,
    analyses: HashMap<String, AnalysisMeta>,
    builds: HashMap<String, BuildMeta>,
    mir_fns: HashMap<String, MirMeta>,
    lru: LruMap<String, CacheEntryKind>,
    metrics: CacheMetrics,
    needs_checkpoint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CacheEntryKind {
    File,
    Analysis,
    Build,
    MirFn,
}

impl IncrementalCache {
    pub fn load_for(source_path: &Path) -> Result<Self> {
        Self::load_with_settings(
            source_path,
            CacheSettings::resolve_for(source_path, CacheOverrides::default())?,
        )
    }

    pub fn load_with_settings(source_path: &Path, settings: CacheSettings) -> Result<Self> {
        let cache_dir = cache_file_path(source_path);

        // Create directory structure
        fs::create_dir_all(&cache_dir).ok();
        fs::create_dir_all(cache_dir.join(INDEX_DIR).join(FILES_INDEX)).ok();
        fs::create_dir_all(cache_dir.join(INDEX_DIR).join(ANALYSES_INDEX)).ok();
        fs::create_dir_all(cache_dir.join(INDEX_DIR).join(BUILDS_INDEX)).ok();
        fs::create_dir_all(cache_dir.join(INDEX_DIR).join(MIR_INDEX)).ok();
        fs::create_dir_all(cache_dir.join(BLOBS_DIR)).ok();
        fs::create_dir_all(cache_dir.join(WAL_DIR)).ok();

        // Write version file if missing
        let version_path = cache_dir.join(VERSION_FILE);
        if !version_path.exists() {
            let _ = fs::write(
                &version_path,
                format!("{NEW_CACHE_FORMAT}\n{NEW_FORMAT_VERSION}\n"),
            );
        }

        // Replay WAL
        let records = replay_wal(&cache_dir).unwrap_or_default();
        let max_units = settings.max_units.unwrap_or(DEFAULT_MAX_UNITS);

        // Load existing metas from disk
        let mut files = HashMap::new();
        let mut analyses = HashMap::new();
        let mut builds = HashMap::new();
        let mut mir_fns = HashMap::new();
        let mut lru = LruMap::new(max_units);

        load_file_metas(&cache_dir, &mut files, &mut lru);
        load_analysis_metas(&cache_dir, &mut analyses, &mut lru);
        load_build_metas(&cache_dir, &mut builds, &mut lru);
        load_mir_metas(&cache_dir, &mut mir_fns, &mut lru);

        // Apply WAL records
        for rec in &records {
            apply_wal_record(rec, &mut files, &mut analyses, &mut builds, &mut mir_fns, &mut lru, &cache_dir);
        }

        Ok(Self {
            cache_dir,
            settings,
            files,
            analyses,
            builds,
            mir_fns,
            lru,
            metrics: CacheMetrics::default(),
            needs_checkpoint: !records.is_empty(),
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let mut wal_records = Vec::new();

        for key in self.files.keys() {
            if let Some(meta) = self.files.get(key) {
                let _ = write_file_meta(&self.cache_dir, key, meta);
                wal_records.push(WalRecord::Checkpoint { timestamp: timestamp_ms() });
            }
        }
        for key in self.analyses.keys() {
            if let Some(meta) = self.analyses.get(key) {
                let _ = write_analysis_meta(&self.cache_dir, key, meta);
            }
        }
        for key in self.builds.keys() {
            if let Some(meta) = self.builds.get(key) {
                let _ = write_build_meta(&self.cache_dir, key, meta);
            }
        }
        for key in self.mir_fns.keys() {
            if let Some(meta) = self.mir_fns.get(key) {
                let _ = write_mir_meta(&self.cache_dir, key, meta);
            }
        }

        // GC unreferenced blobs
        let referenced = collect_referenced_blobs(&self.cache_dir)?;
        gc_blobs(&self.cache_dir, &referenced)?;

        // Clear WAL after successful metadata flush
        clear_wal(&self.cache_dir)?;

        self.needs_checkpoint = false;
        Ok(())
    }

    pub fn metrics(&self) -> &CacheMetrics {
        &self.metrics
    }

    pub fn record_build_hit(&mut self) {
        self.metrics.build_hits += 1;
    }

    pub fn record_build_miss(&mut self) {
        self.metrics.build_misses += 1;
    }

    pub fn cached_file(&mut self, path: &Path, hash: u64, hash2: u64) -> Option<CachedParsedFile> {
        let key = normalize_path_key(path);
        let meta = match self.files.get(&key) {
            Some(m) => m.clone(),
            None => {
                // try loading from disk (cold entry)
                let m = read_file_meta(&self.cache_dir, &key)?;
                if m.hash != hash || m.hash2 != hash2 {
                    self.metrics.file_misses += 1;
                    return None;
                }
                self.files.insert(key.clone(), m.clone());
                m
            }
        };

        if meta.hash != hash || meta.hash2 != hash2 {
            self.metrics.file_misses += 1;
            return None;
        }

        let blob = read_blob(&self.cache_dir, &meta.blob_hash)?;
        let stored: StoredParsedFile = bincode::deserialize(&blob).ok()?;

        self.lru.insert(key, CacheEntryKind::File);
        if let Some(meta) = self.files.get_mut(&normalize_path_key(path)) {
            meta.last_access_ms = timestamp_ms();
        }

        self.metrics.file_hits += 1;
        Some(CachedParsedFile {
            hash,
            hash2,
            program: stored.program,
            exports: stored.exports,
            local_imports: stored.local_imports,
        })
    }

    pub fn store_file(&mut self, path: &Path, entry: CachedParsedFile) -> Result<()> {
        let key = normalize_path_key(path);
        let stored = StoredParsedFile {
            program: entry.program,
            exports: entry.exports,
            local_imports: entry.local_imports,
        };
        let blob = bincode::serialize(&stored).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot serialize cached parsed file: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let meta = FileMeta {
            hash: entry.hash,
            hash2: entry.hash2,
            blob_hash: blob_hash.clone(),
            last_access_ms: timestamp_ms(),
        };

        // WAL
        let wal_rec = WalRecord::StoreFile {
            key: key.clone(),
            hash: entry.hash,
            hash2: entry.hash2,
            blob_hash: blob_hash.clone(),
            timestamp: timestamp_ms(),
        };
        write_wal(&self.cache_dir, &[wal_rec])?;

        self.files.insert(key.clone(), meta);
        self.lru.insert(key, CacheEntryKind::File);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    pub fn cached_analysis(&mut self, source_path: &Path, source_hash: u64) -> Option<CachedAnalysis> {
        if !self.settings.analysis_cache {
            return None;
        }

        let key = analysis_cache_key(source_path, source_hash);
        let meta = match self.analyses.get(&key) {
            Some(m) => m.clone(),
            None => {
                let m = read_analysis_meta(&self.cache_dir, &key)?;
                self.analyses.insert(key.clone(), m.clone());
                m
            }
        };

        let blob = read_blob(&self.cache_dir, &meta.blob_hash)?;
        let stored: StoredAnalysisPayload = bincode::deserialize(&blob).ok()?;

        self.lru.insert(key, CacheEntryKind::Analysis);
        self.metrics.analysis_hits += 1;
        match stored.outcome {
            StoredAnalysisOutcome::Success(s) => Some(CachedAnalysis::Success(s.program)),
            StoredAnalysisOutcome::Error(e) => Some(CachedAnalysis::Error(e.into())),
        }
    }

    pub fn store_analysis(&mut self, source_path: &Path, source_hash: u64, program: &Program) -> Result<()> {
        if !self.settings.analysis_cache {
            return Ok(());
        }

        let key = analysis_cache_key(source_path, source_hash);
        let units = analysis_units_for_program(program);
        let stored = StoredAnalysisPayload {
            outcome: StoredAnalysisOutcome::Success(StoredAnalyzedProgram {
                program: program.clone(),
            }),
            units: units.clone(),
        };
        let blob = bincode::serialize(&stored).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot serialize analysis cache entry: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let wal_rec = WalRecord::StoreAnalysis {
            key: key.clone(),
            fingerprint: 0,
            blob_hash: blob_hash.clone(),
            timestamp: now,
            created_ms: now,
            unit_count: units.len() as u32,
        };
        write_wal(&self.cache_dir, &[wal_rec])?;

        self.analyses.insert(
            key.clone(),
            AnalysisMeta {
                fingerprint: 0,
                blob_hash,
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.lru.insert(key, CacheEntryKind::Analysis);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    pub fn store_analysis_error(
        &mut self,
        source_path: &Path,
        source_hash: u64,
        program: &Program,
        error: &MireError,
    ) -> Result<()> {
        if !self.settings.analysis_cache {
            return Ok(());
        }

        let key = analysis_cache_key(source_path, source_hash);
        let units = analysis_units_for_program(program);
        let stored = StoredAnalysisPayload {
            outcome: StoredAnalysisOutcome::Error(error.into()),
            units: units.clone(),
        };
        let blob = bincode::serialize(&stored).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot serialize analysis error cache entry: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let wal_rec = WalRecord::StoreAnalysis {
            key: key.clone(),
            fingerprint: 0,
            blob_hash: blob_hash.clone(),
            timestamp: now,
            created_ms: now,
            unit_count: units.len() as u32,
        };
        write_wal(&self.cache_dir, &[wal_rec])?;

        self.analyses.insert(
            key.clone(),
            AnalysisMeta {
                fingerprint: 0,
                blob_hash,
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.lru.insert(key, CacheEntryKind::Analysis);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    pub fn analysis_invalidation_report(
        &self,
        source_path: &Path,
        source_hash: u64,
        program: &Program,
    ) -> Option<AnalysisInvalidationReport> {
        let current_units = analysis_units_for_program(program);
        let previous_units = self.latest_analysis_units(source_path, source_hash)?;
        Some(compute_invalidation_report(&previous_units, &current_units))
    }

    pub fn latest_successful_analysis(
        &mut self,
        source_path: &Path,
        source_hash: u64,
    ) -> Option<CachedAnalysisSnapshot> {
        let key = analysis_cache_key(source_path, source_hash);
        let meta = self.analyses.get(&key)?;
        let blob = read_blob(&self.cache_dir, &meta.blob_hash)?;
        let stored: StoredAnalysisPayload = bincode::deserialize(&blob).ok()?;
        let StoredAnalysisOutcome::Success(s) = stored.outcome else {
            return None;
        };
        Some(CachedAnalysisSnapshot {
            program: s.program,
            units: stored.units,
        })
    }

    pub fn build_entry(
        &mut self,
        source_path: &Path,
        mode: BuildMode,
        import_mode: ImportMode,
        emit_binary: bool,
        persist_ir: bool,
    ) -> Option<&BuildCacheEntry> {
        let key = build_cache_key(source_path, mode, import_mode, emit_binary, persist_ir);

        // Check in-memory first
        if !self.builds.contains_key(&key) {
            // Try loading from disk (cold)
            let meta = read_build_meta(&self.cache_dir, &key)?;
            self.lru.insert(key.clone(), CacheEntryKind::Build);
            self.builds.insert(key.clone(), meta);
        } else {
            self.lru.insert(key.clone(), CacheEntryKind::Build);
        }

        self.builds.get(&key).map(|m| &m.entry)
    }

    pub fn store_build(&mut self, source_path: &Path, entry: BuildCacheEntry) {
        let key = build_cache_key(
            source_path,
            entry.mode,
            entry.import_mode,
            entry.emit_binary,
            entry.persist_ir,
        );

        let now = timestamp_ms();
        let meta = BuildMeta {
            fingerprint: entry.fingerprint,
            entry,
            last_access_ms: now,
        };

        let wal_rec = WalRecord::StoreBuild {
            key: key.clone(),
            entry: meta.entry.clone(),
            timestamp: now,
        };
        let _ = write_wal(&self.cache_dir, &[wal_rec]);

        self.builds.insert(key.clone(), meta);
        self.lru.insert(key, CacheEntryKind::Build);
        self.enforce_capacity();
        self.needs_checkpoint = true;
    }

    pub fn get_cached_mir_fn(
        &mut self,
        source_path: &Path,
        fn_name: &str,
        body_hash: u64,
        opt_level: OptLevel,
    ) -> Option<String> {
        let key = mir_cache_key(source_path, fn_name, body_hash, opt_level);
        let meta = match self.mir_fns.get(&key) {
            Some(m) => m.clone(),
            None => {
                let m = read_mir_meta(&self.cache_dir, &key)?;
                if m.body_hash != body_hash {
                    return None;
                }
                self.mir_fns.insert(key.clone(), m.clone());
                m
            }
        };

        if meta.body_hash != body_hash {
            return None;
        }

        let blob = read_blob(&self.cache_dir, &meta.blob_hash)?;
        let ir: String = bincode::deserialize(&blob).ok()?;

        self.lru.insert(key, CacheEntryKind::MirFn);
        if let Some(meta) = self.mir_fns.get_mut(&mir_cache_key(source_path, fn_name, body_hash, opt_level)) {
            meta.last_access_ms = timestamp_ms();
        }

        Some(ir)
    }

    pub fn store_cached_mir_fn(
        &mut self,
        source_path: &Path,
        fn_name: &str,
        body_hash: u64,
        opt_level: OptLevel,
        llvm_ir: &str,
    ) -> Result<()> {
        let key = mir_cache_key(source_path, fn_name, body_hash, opt_level);

        let blob = bincode::serialize(llvm_ir).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Cannot serialize MIR fn IR: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let meta = MirMeta {
            body_hash,
            blob_hash: blob_hash.clone(),
            last_access_ms: now,
        };

        let wal_rec = WalRecord::StoreMirFn {
            key: key.clone(),
            body_hash,
            blob_hash,
            timestamp: now,
        };
        write_wal(&self.cache_dir, &[wal_rec])?;

        self.mir_fns.insert(key.clone(), meta);
        self.lru.insert(key, CacheEntryKind::MirFn);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    fn enforce_capacity(&mut self) {
        let max = self.settings.max_units.unwrap_or(usize::MAX);
        let mut total = self.files.len() + self.analyses.len() + self.builds.len() + self.mir_fns.len();
        while total > max {
            let Some(oldest_key) = self.lru.evict_one() else {
                break;
            };
            if self.files.remove(&oldest_key).is_some() {
                self.metrics.evictions += 1;
            } else if self.analyses.remove(&oldest_key).is_some() {
                self.metrics.evictions += 1;
            } else if self.builds.remove(&oldest_key).is_some() {
                self.metrics.evictions += 1;
            } else if self.mir_fns.remove(&oldest_key).is_some() {
                self.metrics.evictions += 1;
            }
            let new_total = self.files.len() + self.analyses.len() + self.builds.len() + self.mir_fns.len();
            if new_total >= total {
                break;
            }
            total = new_total;
        }
    }

    // ── Test / compat accessors ──────────────────────────────────────────

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn analysis_count(&self) -> usize {
        self.analyses.len()
    }

    pub fn build_count(&self) -> usize {
        self.builds.len()
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn latest_analysis_units(&self, source_path: &Path, source_hash: u64) -> Option<Vec<AnalysisUnitMetadata>> {
        let key = analysis_cache_key(source_path, source_hash);
        let meta = self.analyses.get(&key)?;
        let blob = read_blob(&self.cache_dir, &meta.blob_hash)?;
        let stored: StoredAnalysisPayload = bincode::deserialize(&blob).ok()?;
        Some(stored.units)
    }
}

// ── Helper functions ────────────────────────────────────────────────────

    fn load_file_metas(
    base_dir: &Path,
    files: &mut HashMap<String, FileMeta>,
    _lru: &mut LruMap<String, CacheEntryKind>,
) {
    let dir = base_dir.join(INDEX_DIR).join(FILES_INDEX);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(content) = fs::read_to_string(entry.path())
            && let Ok(meta) = serde_json::from_str::<FileMeta>(&content) {
                // Derive key from filename (hash)
                if let Some(name) = entry.file_name().to_str()
                    && let Some(stem) = name.strip_suffix(".meta") {
                        files.insert(stem.to_string(), meta);
                    }
            }
    }
}

fn load_analysis_metas(
    base_dir: &Path,
    analyses: &mut HashMap<String, AnalysisMeta>,
    _lru: &mut LruMap<String, CacheEntryKind>,
) {
    let dir = base_dir.join(INDEX_DIR).join(ANALYSES_INDEX);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(content) = fs::read_to_string(entry.path())
            && let Ok(meta) = serde_json::from_str::<AnalysisMeta>(&content)
                && let Some(name) = entry.file_name().to_str()
                    && let Some(stem) = name.strip_suffix(".meta") {
                        analyses.insert(stem.to_string(), meta);
                    }
    }
}

fn load_build_metas(
    base_dir: &Path,
    builds: &mut HashMap<String, BuildMeta>,
    _lru: &mut LruMap<String, CacheEntryKind>,
) {
    let dir = base_dir.join(INDEX_DIR).join(BUILDS_INDEX);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(content) = fs::read_to_string(entry.path())
            && let Ok(meta) = serde_json::from_str::<BuildMeta>(&content)
                && let Some(name) = entry.file_name().to_str()
                    && let Some(stem) = name.strip_suffix(".meta") {
                        builds.insert(stem.to_string(), meta);
                    }
    }
}

fn load_mir_metas(
    base_dir: &Path,
    mir_fns: &mut HashMap<String, MirMeta>,
    _lru: &mut LruMap<String, CacheEntryKind>,
) {
    let dir = base_dir.join(INDEX_DIR).join(MIR_INDEX);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(content) = fs::read_to_string(entry.path())
            && let Ok(meta) = serde_json::from_str::<MirMeta>(&content)
                && let Some(name) = entry.file_name().to_str()
                    && let Some(stem) = name.strip_suffix(".meta") {
                        mir_fns.insert(stem.to_string(), meta);
                    }
    }
}

fn apply_wal_record(
    rec: &WalRecord,
    files: &mut HashMap<String, FileMeta>,
    analyses: &mut HashMap<String, AnalysisMeta>,
    builds: &mut HashMap<String, BuildMeta>,
    mir_fns: &mut HashMap<String, MirMeta>,
    lru: &mut LruMap<String, CacheEntryKind>,
    _base_dir: &Path,
) {
    match rec {
        WalRecord::StoreFile {
            key,
            hash,
            hash2,
            blob_hash,
            timestamp,
        } => {
            let meta = FileMeta {
                hash: *hash,
                hash2: *hash2,
                blob_hash: blob_hash.clone(),
                last_access_ms: *timestamp,
            };
            files.insert(key.clone(), meta);
            lru.insert(key.clone(), CacheEntryKind::File);
        }
        WalRecord::StoreAnalysis {
            key,
            fingerprint,
            blob_hash,
            timestamp,
            created_ms,
            unit_count,
        } => {
            let meta = AnalysisMeta {
                fingerprint: *fingerprint,
                blob_hash: blob_hash.clone(),
                last_access_ms: *timestamp,
                created_ms: *created_ms,
                unit_count: *unit_count,
            };
            analyses.insert(key.clone(), meta);
            lru.insert(key.clone(), CacheEntryKind::Analysis);
        }
        WalRecord::StoreBuild { key, entry, timestamp } => {
            let meta = BuildMeta {
                fingerprint: entry.fingerprint,
                entry: entry.clone(),
                last_access_ms: *timestamp,
            };
            builds.insert(key.clone(), meta);
            lru.insert(key.clone(), CacheEntryKind::Build);
        }
        WalRecord::DeleteFile { key, .. } => {
            files.remove(key);
            lru.remove(key);
        }
        WalRecord::DeleteAnalysis { key, .. } => {
            analyses.remove(key);
            lru.remove(key);
        }
        WalRecord::DeleteBuild { key, .. } => {
            builds.remove(key);
            lru.remove(key);
        }
        WalRecord::StoreMirFn {
            key,
            body_hash,
            blob_hash,
            timestamp,
        } => {
            let meta = MirMeta {
                body_hash: *body_hash,
                blob_hash: blob_hash.clone(),
                last_access_ms: *timestamp,
            };
            mir_fns.insert(key.clone(), meta);
            lru.insert(key.clone(), CacheEntryKind::MirFn);
        }
        WalRecord::DeleteMirFn { key, .. } => {
            mir_fns.remove(key);
            lru.remove(key);
        }
        WalRecord::Checkpoint { .. } => {}
    }
}
