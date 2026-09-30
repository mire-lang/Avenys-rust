use super::cache_types::{AnalysisMeta, BuildMeta, FileMeta, MirMeta, WalRecord};
use super::lru::LruMap;
use super::*;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Concurrency-safe cache I/O ──────────────────────────────────────────
//
// The cache directory is shared by every worker in a parallel `mire test`
// run (one `IncrementalCache` instance per thread, all pointing at the same
// `bin/.cache`), and can also be shared across processes (a project and the
// projects that depend on it). Every on-disk mutation therefore has to be:
//
//   1. collision-free in naming   — WAL filenames embed pid + sequence so two
//      writers can never truncate each other's file,
//   2. atomic on write            — temp file + `fs::rename`, so a reader
//      never observes a partially written meta/blob,
//   3. tolerant on read           — a corrupt/truncated WAL line is dropped
//      and the file removed instead of failing the whole load, and
//   4. conservative on cleanup    — WAL files and blobs are only removed when
//      they can no longer belong to an in-flight writer.

static WAL_SEQ: AtomicU64 = AtomicU64::new(0);
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

const WAL_GRACE_SECS: u64 = 60;
const BLOB_GRACE_SECS: u64 = 30;

const VERSION_FILE: &str = "version.txt";
const INDEX_DIR: &str = "index";
const BLOBS_DIR: &str = "blobs";
const WAL_DIR: &str = "wal";
const NEW_CACHE_FORMAT: &str = "MIREINC4";
/// Bump this whenever the compiler changes analysis/MIR/codegen semantics so
/// that stale incremental entries are invalidated instead of silently reused.
/// Bumped to 4 for the concurrency-hardened WAL layout ({ts}-{pid}-{seq}.wal)
/// and to 5 because test-mode builds previously persisted the harness-injected
/// program under the normal analysis key (a later `owl run` would execute the
/// test runner instead of the real `main`); cached entries may be polluted.
const NEW_FORMAT_VERSION: u32 = 5;
const FILES_INDEX: &str = "files";
const ANALYSES_INDEX: &str = "analyses";
const BUILDS_INDEX: &str = "builds";
const MIR_INDEX: &str = "mir";
// ── WAL helpers ─────────────────────────────────────────────────────────

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Writes one WAL file containing all `records` and returns its path.
///
/// The filename embeds a process id and a per-process sequence number so that
/// concurrent writers (parallel test threads, or separate processes sharing a
/// cache dir) can never collide on the same path. The file is opened with
/// `create_new`, so an existing path is bumped to a fresh sequence number
/// instead of being truncated.
fn write_wal(base_dir: &Path, records: &[WalRecord]) -> Result<PathBuf> {
    let wal_dir = base_dir.join(WAL_DIR);
    fs::create_dir_all(&wal_dir).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot create WAL dir: {e}"),
        })
    })?;
    loop {
        let seq = WAL_SEQ.fetch_add(1, Ordering::Relaxed);
        let candidate = wal_dir.join(format!(
            "{}-{}-{}.wal",
            timestamp_ms(),
            std::process::id(),
            seq
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                for rec in records {
                    let line = serde_json::to_string(rec).map_err(|e| {
                        MireError::new(ErrorKind::Runtime {
                            span: crate::error::Span::unknown(),
                            message: format!("Cannot serialize WAL record: {e}"),
                        })
                    })?;
                    writeln!(file, "{line}").map_err(|e| {
                        MireError::new(ErrorKind::Runtime {
                            span: crate::error::Span::unknown(),
                            message: format!("Cannot write WAL record: {e}"),
                        })
                    })?;
                }
                file.sync_all().map_err(|e| {
                    MireError::new(ErrorKind::Runtime {
                        span: crate::error::Span::unknown(),
                        message: format!("Cannot flush WAL file: {e}"),
                    })
                })?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(MireError::new(ErrorKind::Runtime {
                    span: crate::error::Span::unknown(),
                    message: format!("Cannot create WAL file: {e}"),
                }));
            }
        }
    }
}

fn replay_wal(base_dir: &Path) -> Result<Vec<WalRecord>> {
    let wal_dir = base_dir.join(WAL_DIR);
    if !wal_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = fs::read_dir(&wal_dir)
        .map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Cannot read WAL dir: {e}"),
            })
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "wal"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut all_records = Vec::new();
    let mut to_remove: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry.path();
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[AVENYS] incremental: dropping unreadable WAL file '{}': {e}",
                    path.display()
                );
                to_remove.push(path);
                continue;
            }
        };
        for (line_number, line) in content.lines().enumerate() {
            match serde_json::from_str::<WalRecord>(line) {
                Ok(record) => all_records.push(record),
                Err(e) => {
                    // A truncated/interleaved WAL line (e.g. from an
                    // interrupted write) must never brick the whole cache:
                    // drop the bad file so the next load starts clean and
                    // keep the records that decoded so far.
                    eprintln!(
                        "[AVENYS] incremental: ignoring corrupt WAL file '{}' at line {}: {e}",
                        path.display(),
                        line_number + 1
                    );
                    to_remove.push(path.clone());
                    break;
                }
            }
        }
    }
    for path in to_remove {
        let _ = fs::remove_file(&path);
    }
    Ok(all_records)
}

/// Removes WAL files older than [`WAL_GRACE_SECS`]. Such files were written
/// by a long-finished writer: either its `save()` checkpointed the records
/// (and it dropped its own files) or it crashed (records are lost, which only
/// costs a recompute). This bounds WAL growth without ever touching a live
/// writer's in-flight file.
fn prune_stale_wal(base_dir: &Path) {
    let wal_dir = base_dir.join(WAL_DIR);
    let Ok(entries) = fs::read_dir(&wal_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if timestamp_ms().saturating_sub(mtime) > WAL_GRACE_SECS * 1000 {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Creates the full cache directory structure: the four index subdirectories
/// plus the blob and WAL stores. Idempotent, and safe to call from a thread
/// that did not win the init lock, because by then the only thread allowed to
/// wipe has already finished.
fn create_cache_dirs(cache_dir: &Path) {
    fs::create_dir_all(cache_dir).ok();
    fs::create_dir_all(cache_dir.join(INDEX_DIR).join(FILES_INDEX)).ok();
    fs::create_dir_all(cache_dir.join(INDEX_DIR).join(ANALYSES_INDEX)).ok();
    fs::create_dir_all(cache_dir.join(INDEX_DIR).join(BUILDS_INDEX)).ok();
    fs::create_dir_all(cache_dir.join(INDEX_DIR).join(MIR_INDEX)).ok();
    fs::create_dir_all(cache_dir.join(BLOBS_DIR)).ok();
    fs::create_dir_all(cache_dir.join(WAL_DIR)).ok();
}

/// Name of the directory used as the cache-initialisation lock.
const INIT_LOCK_NAME: &str = ".init.lock";

/// Removes all cache contents (indexes, blobs, WAL, version file) so a fresh
/// cache can be rebuilt after a compiler version change.
///
/// The init lock is deliberately preserved. A `remove_dir_all` here used to take
/// the lock with it, so the thread holding it went on believing it was
/// exclusive while another thread created the lock directory again and started
/// a second wipe — which is how a cold `mire test -j N` ended up deleting
/// `blobs/` from under a thread that was mid-write and failing with
/// `Cannot write cache file ...: No such file or directory`. Callers must hold
/// the lock; keeping it is what makes that mutual exclusion real.
fn wipe_cache_dir(cache_dir: &Path) {
    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            if entry.file_name() == std::ffi::OsStr::new(INIT_LOCK_NAME) {
                continue;
            }
            let path = entry.path();
            // Not `remove_dir_all` on the cache dir itself: that is exactly what
            // used to destroy the lock.
            let _ = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
        }
    }
    create_cache_dirs(cache_dir);
}

const INIT_LOCK_STALE_SECS: u64 = 30;

/// The init lock path for a cache directory.
///
/// It lives inside `cache_dir` — which is already a build artifact directory
/// (`bin/.cache`, or `$MIRE_CACHE_DIR`) that projects ignore — so no lock
/// bookkeeping leaks into the project tree. `wipe_cache_dir` skips it, which is
/// what stops a wipe from taking the lock that guards it.
fn init_lock_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(INIT_LOCK_NAME)
}

/// Removes the init lock when dropped (only the thread that created it).
struct InitLockGuard<'a>(&'a Path);

impl Drop for InitLockGuard<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir(self.0);
    }
}

/// True when `cache_dir` already holds a version file matching this build.
///
/// The version file stores two lines: the format tag (e.g. "MIREINC4") on line
/// 1 and the version number (e.g. "5") on line 2. Parsing the whole file as a
/// single `u32` would always fail, because the tag is not numeric, so each line
/// is read independently.
fn cache_version_is_current(cache_dir: &Path) -> bool {
    let Ok(content) = fs::read_to_string(cache_dir.join(VERSION_FILE)) else {
        return false;
    };
    let mut lines = content.lines();
    let format = lines.next().map(str::trim);
    let version = lines.last().map(str::trim).and_then(|v| v.parse::<u32>().ok());
    format == Some(NEW_CACHE_FORMAT) && version == Some(NEW_FORMAT_VERSION)
}

/// Initialises `cache_dir`. Callers must hold the init lock.
///
/// Ordering matters and is the whole point of this function: any wipe happens
/// *first*, and the directory structure is rebuilt *after* it. Creating the
/// directories before the wipe left a window where the wipe removed a
/// directory a sibling thread had just made, and that sibling then failed a
/// write against a path it had already created.
fn init_cache(cache_dir: &Path) {
    // A wipe is destructive by definition, so it is only safe while exclusive.
    // Asserted rather than documented because getting this wrong reproduces the
    // original CI failure (ENOENT on a cache write) rather than failing
    // cleanly, and every caller reaching here on a path that did not take the
    // lock first has silently reintroduced the race.
    debug_assert!(
        init_lock_path(cache_dir).is_dir(),
        "init_cache() called without holding the init lock; its wipe would race \
         with every other writer sharing this cache"
    );
    if !cache_version_is_current(cache_dir) {
        wipe_cache_dir(cache_dir);
    }
    create_cache_dirs(cache_dir);
    let _ = atomic_write(
        &cache_dir.join(VERSION_FILE),
        format!("{NEW_CACHE_FORMAT}\n{NEW_FORMAT_VERSION}\n").as_bytes(),
    );

    // Bound WAL growth left behind by abandoned writers before replaying.
    prune_stale_wal(cache_dir);
}

/// Waits until `cache_dir` is genuinely initialised, then returns.
///
/// The previous implementation merely waited for the lock directory to vanish
/// and returned. That is not the same thing: a holder that dies between its
/// wipe and its version write leaves the lock gone but the cache unversioned,
/// and the waiter would carry on into a cache that the next arriving thread
/// would then wipe underneath it. So the exit condition is the version file
/// itself, and a waiter that finds the lock free (or stale, because a holder
/// crashed) takes the lock and performs the init rather than assuming someone
/// else did.
fn wait_for_cache_init(cache_dir: &Path, lock_dir: &Path) {
    let threshold = std::time::Duration::from_secs(INIT_LOCK_STALE_SECS);
    wait_for_cache_init_with(cache_dir, lock_dir, threshold, threshold);
}

/// [`wait_for_cache_init`] with both timing thresholds injected, so tests can
/// exercise lock stealing and the give-up path without waiting 30 seconds.
fn wait_for_cache_init_with(
    cache_dir: &Path,
    lock_dir: &Path,
    stale_after: std::time::Duration,
    give_up_after: std::time::Duration,
) {
    let mut deadline = std::time::Instant::now() + give_up_after;
    loop {
        if cache_version_is_current(cache_dir) {
            return;
        }
        // The lock is free, or its holder is gone. Either way, own the lock and
        // do the init ourselves rather than waiting for someone who may never
        // come back. `create_dir` is the only thing that grants the lock, so
        // exclusivity rests on it alone.
        if fs::create_dir(lock_dir).is_ok() {
            let _guard = InitLockGuard(lock_dir);
            init_cache(cache_dir);
            return;
        }
        // Still held. Break the lock if the holder outlived its welcome.
        if let Some(age) = fs::metadata(lock_dir)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.elapsed().ok())
            && age > stale_after
        {
            let _ = fs::remove_dir(lock_dir);
            continue;
        }
        if std::time::Instant::now() >= deadline {
            // Last resort: the lock is held but its age cannot be read (an
            // mtime in the future, or a filesystem that will not stat it), so
            // the branch above can never make progress on its own. Break the
            // lock and go round again to re-acquire it properly.
            //
            // Deliberately *not* an unsynchronised `init_cache` here. Removing
            // someone else's lock does not make the cache ours, and wiping it
            // without holding the lock is precisely the failure this whole
            // protocol exists to prevent — it is how the CI race started.
            // Re-arming the deadline keeps the retries bounded while leaving
            // the invariant intact.
            let _ = fs::remove_dir(lock_dir);
            deadline = std::time::Instant::now() + give_up_after;
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Writes `bytes` to `path` atomically: write to a unique temp file in the
/// same directory, then `fs::rename` over the destination. Concurrent readers
/// never observe a partially written file, and concurrent writers each commit
/// complete content (last writer wins; content-addressed blobs make the value
/// identical anyway).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "entry".to_string());
    let tmp = path.with_file_name(format!(
        ".{}.tmp.{}",
        name,
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(e) = fs::write(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot write cache file '{}': {e}", path.display()),
        }));
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot commit cache file '{}': {e}", path.display()),
        }));
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
            span: crate::error::Span::unknown(),
            message: format!("Cannot create blobs dir: {e}"),
        })
    })?;
    let path = blob_dir.join(&hash);
    if !path.exists() {
        atomic_write(&path, blob)?;
    }
    Ok(hash)
}

fn read_blob(base_dir: &Path, blob_hash: &str, verify: bool) -> Option<Vec<u8>> {
    let path = base_dir.join(BLOBS_DIR).join(blob_hash);
    let blob = fs::read(&path).ok()?;
    if verify && compute_blob_hash(&blob) != blob_hash {
        // Cache poisoning / on-disk corruption: drop the blob so the next
        // `store_blob` rewrites it (store_blob skips existing files), and
        // treat this entry as a miss instead of trusting the tampered bytes.
        let _ = fs::remove_file(&path);
        return None;
    }
    Some(blob)
}

fn gc_blobs(base_dir: &Path, referenced: &HashSet<String>) -> Result<()> {
    let blob_dir = base_dir.join(BLOBS_DIR);
    if !blob_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&blob_dir).ok().into_iter().flatten() {
        if let Ok(e) = entry
            && let Some(name) = e.file_name().to_str()
            && !referenced.contains(name)
        {
            // Only collect blobs that have been unreferenced long enough that
            // no concurrent writer can still be about to reference them (a
            // writer stores the blob before recording its meta/WAL entry).
            let stale = fs::metadata(e.path())
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| {
                    timestamp_ms().saturating_sub(d.as_millis() as u64) > BLOB_GRACE_SECS * 1000
                })
                .unwrap_or(false);
            if stale {
                let _ = fs::remove_file(e.path());
            }
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
    let mut meta = meta.clone();
    meta.key = key.to_string();
    let json = serde_json::to_string(&meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot serialize file meta: {e}"),
        })
    })?;
    atomic_write(&path, json.as_bytes())
}

fn read_file_meta(base_dir: &Path, key: &str) -> Option<FileMeta> {
    let path = file_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_analysis_meta(base_dir: &Path, key: &str, meta: &AnalysisMeta) -> Result<()> {
    let path = analysis_meta_path(base_dir, key);
    let mut meta = meta.clone();
    meta.key = key.to_string();
    let json = serde_json::to_string(&meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot serialize analysis meta: {e}"),
        })
    })?;
    atomic_write(&path, json.as_bytes())
}

fn read_analysis_meta(base_dir: &Path, key: &str) -> Option<AnalysisMeta> {
    let path = analysis_meta_path(base_dir, key);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_build_meta(base_dir: &Path, key: &str, meta: &BuildMeta) -> Result<()> {
    let path = build_meta_path(base_dir, key);
    let mut meta = meta.clone();
    meta.key = key.to_string();
    let json = serde_json::to_string(&meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot serialize build meta: {e}"),
        })
    })?;
    atomic_write(&path, json.as_bytes())
}

fn write_mir_meta(base_dir: &Path, key: &str, meta: &MirMeta) -> Result<()> {
    let path = mir_meta_path(base_dir, key);
    let mut meta = meta.clone();
    meta.key = key.to_string();
    let json = serde_json::to_string(&meta).map_err(|e| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Cannot serialize mir meta: {e}"),
        })
    })?;
    atomic_write(&path, json.as_bytes())
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
                && let Ok(meta) = serde_json::from_str::<FileMeta>(&content)
            {
                referenced.insert(meta.blob_hash);
            }
        }
    }

    // scan analysis metas
    let analyses_dir = base_dir.join(INDEX_DIR).join(ANALYSES_INDEX);
    if let Ok(entries) = fs::read_dir(&analyses_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(meta) = serde_json::from_str::<AnalysisMeta>(&content)
            {
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
                && let Ok(meta) = serde_json::from_str::<MirMeta>(&content)
            {
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
    /// WAL files this instance created during this run. `save()` drops only
    /// these after checkpointing; WAL files from concurrent writers are left
    /// for their owners (and eventually pruned by age).
    wal_written: Vec<PathBuf>,
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

        // Validate the cache format/version and build the directory structure
        // under an exclusive init lock. See `init_cache` / `wait_for_cache_init`
        // for why `wipe_cache_dir` must skip the lock and why a waiter must not
        // return until the cache is genuinely initialized.
        fs::create_dir_all(&cache_dir).ok();
        let init_lock = init_lock_path(&cache_dir);
        if fs::create_dir(&init_lock).is_ok() {
            let _guard = InitLockGuard(&init_lock);
            init_cache(&cache_dir);
        } else {
            wait_for_cache_init(&cache_dir, &init_lock);
        }

        // Replay WAL
        let records = replay_wal(&cache_dir)?;
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
            apply_wal_record(
                rec,
                &mut files,
                &mut analyses,
                &mut builds,
                &mut mir_fns,
                &mut lru,
                &cache_dir,
            );
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
            wal_written: Vec::new(),
        })
    }

    pub fn save(&mut self) -> Result<()> {
        for key in self.files.keys() {
            if let Some(meta) = self.files.get(key) {
                write_file_meta(&self.cache_dir, key, meta)?;
            }
        }
        for key in self.analyses.keys() {
            if let Some(meta) = self.analyses.get(key) {
                write_analysis_meta(&self.cache_dir, key, meta)?;
            }
        }
        for key in self.builds.keys() {
            if let Some(meta) = self.builds.get(key) {
                write_build_meta(&self.cache_dir, key, meta)?;
            }
        }
        for key in self.mir_fns.keys() {
            if let Some(meta) = self.mir_fns.get(key) {
                write_mir_meta(&self.cache_dir, key, meta)?;
            }
        }

        // GC unreferenced blobs (age-gated so concurrent writers are never
        // racing their own freshly-stored blob against this scan).
        let referenced = collect_referenced_blobs(&self.cache_dir)?;
        gc_blobs(&self.cache_dir, &referenced)?;

        // Drop only the WAL files this instance created. Concurrent writers'
        // WAL files are left alone and pruned by age once abandoned.
        for path in self.wal_written.drain(..) {
            let _ = fs::remove_file(&path);
        }

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

        let blob = read_blob(
            &self.cache_dir,
            &meta.blob_hash,
            self.settings.blob_checksum,
        )?;
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
                span: crate::error::Span::unknown(),
                message: format!("Cannot serialize cached parsed file: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let meta = FileMeta {
            key: key.clone(),
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
        if let Ok(p) = write_wal(&self.cache_dir, &[wal_rec]) {
            self.wal_written.push(p);
        }

        self.files.insert(key.clone(), meta);
        self.lru.insert(key, CacheEntryKind::File);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    pub fn cached_analysis(
        &mut self,
        source_path: &Path,
        source_hash: u64,
        dep_fingerprint: u64,
    ) -> Option<CachedAnalysis> {
        if !self.settings.analysis_cache {
            return None;
        }

        let key = analysis_cache_key(source_path, source_hash, dep_fingerprint);
        let meta = match self.analyses.get(&key) {
            Some(m) => m.clone(),
            None => {
                let m = read_analysis_meta(&self.cache_dir, &key)?;
                self.analyses.insert(key.clone(), m.clone());
                m
            }
        };

        let blob = read_blob(
            &self.cache_dir,
            &meta.blob_hash,
            self.settings.blob_checksum,
        )?;
        let stored: StoredAnalysisPayload = bincode::deserialize(&blob).ok()?;

        self.lru.insert(key, CacheEntryKind::Analysis);
        self.metrics.analysis_hits += 1;
        match stored.outcome {
            StoredAnalysisOutcome::Success(s) => Some(CachedAnalysis::Success(s.program)),
            StoredAnalysisOutcome::Error(e) => Some(CachedAnalysis::Error(e.into())),
        }
    }

    pub fn store_analysis(
        &mut self,
        source_path: &Path,
        source_hash: u64,
        dep_fingerprint: u64,
        program: &Program,
    ) -> Result<()> {
        if !self.settings.analysis_cache {
            return Ok(());
        }

        let key = analysis_cache_key(source_path, source_hash, dep_fingerprint);
        let latest_key = latest_analysis_key(source_path);
        let units = analysis_units_for_program(program);
        let stored = StoredAnalysisPayload {
            outcome: StoredAnalysisOutcome::Success(StoredAnalyzedProgram {
                program: program.clone(),
            }),
            units: units.clone(),
        };
        let blob = bincode::serialize(&stored).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Cannot serialize analysis cache entry: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let wal_rec = WalRecord::StoreAnalysis {
            key: key.clone(),
            fingerprint: dep_fingerprint,
            blob_hash: blob_hash.clone(),
            timestamp: now,
            created_ms: now,
            unit_count: units.len() as u32,
        };
        if let Ok(p) = write_wal(&self.cache_dir, &[wal_rec]) {
            self.wal_written.push(p);
        }

        self.analyses.insert(
            key.clone(),
            AnalysisMeta {
                key: key.clone(),
                fingerprint: dep_fingerprint,
                blob_hash: blob_hash.clone(),
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.analyses.insert(
            latest_key.clone(),
            AnalysisMeta {
                key: latest_key.clone(),
                fingerprint: dep_fingerprint,
                blob_hash,
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.lru.insert(key, CacheEntryKind::Analysis);
        self.lru.insert(latest_key, CacheEntryKind::Analysis);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    pub fn store_analysis_error(
        &mut self,
        source_path: &Path,
        source_hash: u64,
        dep_fingerprint: u64,
        program: &Program,
        error: &MireError,
    ) -> Result<()> {
        if !self.settings.analysis_cache {
            return Ok(());
        }

        let key = analysis_cache_key(source_path, source_hash, dep_fingerprint);
        let latest_key = latest_analysis_key(source_path);
        let units = analysis_units_for_program(program);
        let stored = StoredAnalysisPayload {
            outcome: StoredAnalysisOutcome::Error(error.into()),
            units: units.clone(),
        };
        let blob = bincode::serialize(&stored).map_err(|e| {
            MireError::new(ErrorKind::Runtime {
                span: crate::error::Span::unknown(),
                message: format!("Cannot serialize analysis error cache entry: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let wal_rec = WalRecord::StoreAnalysis {
            key: key.clone(),
            fingerprint: dep_fingerprint,
            blob_hash: blob_hash.clone(),
            timestamp: now,
            created_ms: now,
            unit_count: units.len() as u32,
        };
        if let Ok(p) = write_wal(&self.cache_dir, &[wal_rec]) {
            self.wal_written.push(p);
        }

        self.analyses.insert(
            key.clone(),
            AnalysisMeta {
                key: key.clone(),
                fingerprint: dep_fingerprint,
                blob_hash: blob_hash.clone(),
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.analyses.insert(
            latest_key.clone(),
            AnalysisMeta {
                key: latest_key.clone(),
                fingerprint: dep_fingerprint,
                blob_hash,
                last_access_ms: now,
                created_ms: now,
                unit_count: units.len() as u32,
            },
        );
        self.lru.insert(key, CacheEntryKind::Analysis);
        self.lru.insert(latest_key, CacheEntryKind::Analysis);
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
        _source_hash: u64,
    ) -> Option<CachedAnalysisSnapshot> {
        let key = latest_analysis_key(source_path);
        let meta = self.analyses.get(&key)?;
        let blob = read_blob(
            &self.cache_dir,
            &meta.blob_hash,
            self.settings.blob_checksum,
        )?;
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
        test_mode: bool,
    ) -> Option<&BuildCacheEntry> {
        let key = build_cache_key(
            source_path,
            mode,
            import_mode,
            emit_binary,
            persist_ir,
            test_mode,
        );

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

    pub fn store_build(&mut self, source_path: &Path, entry: BuildCacheEntry, test_mode: bool) {
        let key = build_cache_key(
            source_path,
            entry.mode,
            entry.import_mode,
            entry.emit_binary,
            entry.persist_ir,
            test_mode,
        );

        let now = timestamp_ms();
        let meta = BuildMeta {
            key: key.clone(),
            fingerprint: entry.fingerprint,
            entry,
            last_access_ms: now,
        };

        let wal_rec = WalRecord::StoreBuild {
            key: key.clone(),
            entry: meta.entry.clone(),
            timestamp: now,
        };
        if let Ok(p) = write_wal(&self.cache_dir, &[wal_rec]) {
            self.wal_written.push(p);
        }

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

        let blob = read_blob(
            &self.cache_dir,
            &meta.blob_hash,
            self.settings.blob_checksum,
        )?;
        let ir: String = bincode::deserialize(&blob).ok()?;

        self.lru.insert(key, CacheEntryKind::MirFn);
        if let Some(meta) =
            self.mir_fns
                .get_mut(&mir_cache_key(source_path, fn_name, body_hash, opt_level))
        {
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
                span: crate::error::Span::unknown(),
                message: format!("Cannot serialize MIR fn IR: {e}"),
            })
        })?;
        let blob_hash = store_blob(&self.cache_dir, &blob)?;

        let now = timestamp_ms();
        let meta = MirMeta {
            key: key.clone(),
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
        if let Ok(p) = write_wal(&self.cache_dir, &[wal_rec]) {
            self.wal_written.push(p);
        }

        self.mir_fns.insert(key.clone(), meta);
        self.lru.insert(key, CacheEntryKind::MirFn);
        self.enforce_capacity();
        self.needs_checkpoint = true;
        Ok(())
    }

    fn enforce_capacity(&mut self) {
        let max = self.settings.max_units.unwrap_or(usize::MAX);
        let mut total =
            self.files.len() + self.analyses.len() + self.builds.len() + self.mir_fns.len();
        while total > max {
            let Some(oldest_key) = self.lru.evict_one() else {
                break;
            };
            if self.files.remove(&oldest_key).is_some()
                || self.analyses.remove(&oldest_key).is_some()
                || self.builds.remove(&oldest_key).is_some()
                || self.mir_fns.remove(&oldest_key).is_some()
            {
                self.metrics.evictions += 1;
            }
            let new_total =
                self.files.len() + self.analyses.len() + self.builds.len() + self.mir_fns.len();
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

    fn latest_analysis_units(
        &self,
        source_path: &Path,
        _source_hash: u64,
    ) -> Option<Vec<AnalysisUnitMetadata>> {
        let key = latest_analysis_key(source_path);
        let meta = self.analyses.get(&key)?;
        let blob = read_blob(
            &self.cache_dir,
            &meta.blob_hash,
            self.settings.blob_checksum,
        )?;
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
            && let Ok(mut meta) = serde_json::from_str::<FileMeta>(&content)
        {
            // Recover the original key stored in the meta. Old meta files
            // (without the `key` field) fall back to the hashed filename stem.
            if meta.key.is_empty()
                && let Some(name) = entry.file_name().to_str()
                && let Some(stem) = name.strip_suffix(".meta")
            {
                meta.key = stem.to_string();
            }
            files.insert(meta.key.clone(), meta);
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
            && let Ok(mut meta) = serde_json::from_str::<AnalysisMeta>(&content)
        {
            if meta.key.is_empty()
                && let Some(name) = entry.file_name().to_str()
                && let Some(stem) = name.strip_suffix(".meta")
            {
                meta.key = stem.to_string();
            }
            analyses.insert(meta.key.clone(), meta);
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
            && let Ok(mut meta) = serde_json::from_str::<BuildMeta>(&content)
        {
            if meta.key.is_empty()
                && let Some(name) = entry.file_name().to_str()
                && let Some(stem) = name.strip_suffix(".meta")
            {
                meta.key = stem.to_string();
            }
            builds.insert(meta.key.clone(), meta);
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
            && let Ok(mut meta) = serde_json::from_str::<MirMeta>(&content)
        {
            if meta.key.is_empty()
                && let Some(name) = entry.file_name().to_str()
                && let Some(stem) = name.strip_suffix(".meta")
            {
                meta.key = stem.to_string();
            }
            mir_fns.insert(meta.key.clone(), meta);
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
                key: key.clone(),
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
                key: key.clone(),
                fingerprint: *fingerprint,
                blob_hash: blob_hash.clone(),
                last_access_ms: *timestamp,
                created_ms: *created_ms,
                unit_count: *unit_count,
            };
            analyses.insert(key.clone(), meta);
            lru.insert(key.clone(), CacheEntryKind::Analysis);
        }
        WalRecord::StoreBuild {
            key,
            entry,
            timestamp,
        } => {
            let meta = BuildMeta {
                key: key.clone(),
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
                key: key.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("avenys_wal_{tag}_{}", std::process::id()))
    }

    fn wal_record(key: &str) -> WalRecord {
        WalRecord::StoreFile {
            key: key.to_string(),
            hash: 1,
            hash2: 1,
            blob_hash: "deadbeef".to_string(),
            timestamp: timestamp_ms(),
        }
    }

    #[test]
    fn wal_filenames_are_collision_free_across_writes() {
        // Regression for the same-ms {timestamp}.wal truncation: back-to-back
        // writes (which will usually share the millisecond) must never produce
        // the same filename, so a concurrent writer can't truncate ours.
        let dir = tmp_dir("collision");
        let _ = fs::remove_dir_all(&dir);
        let mut paths = Vec::new();
        for i in 0..50 {
            let rec = wal_record(&format!("key_{i}"));
            let p = write_wal(&dir, &[rec]).expect("write wal");
            assert!(
                !paths.contains(&p),
                "duplicate WAL path produced: {}",
                p.display()
            );
            paths.push(p);
        }
        // All records survive replay.
        let records = replay_wal(&dir).expect("replay wal");
        assert_eq!(records.len(), 50);

        // Each writer also clears only its own files on checkpoint; the whole
        // dir must be empty after all are dropped.
        for p in &paths {
            let _ = fs::remove_file(p);
        }
        let remaining: Vec<_> = fs::read_dir(dir.join(WAL_DIR))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(remaining.is_empty(), "WAL dir not empty after cleanup");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replay_wal_ignores_corrupt_and_truncated_files() {
        // Regression for the hard load failure on a truncated WAL line:
        // a corrupt file must be dropped (with a warning) and the valid
        // records from other files must still be replayed.
        let dir = tmp_dir("corrupt");
        let _ = fs::remove_dir_all(&dir);
        let good = write_wal(&dir, &[wal_record("good_key")]).expect("good wal");

        // Simulate the old race: a file truncated mid-JSON.
        let bad = dir.join(WAL_DIR).join("corrupt.wal");
        let mut bytes = serde_json::to_string(&wal_record("bad_key"))
            .unwrap()
            .into_bytes();
        bytes.truncate(bytes.len() / 2);
        fs::write(&bad, bytes).unwrap();

        // A wholly unreadable file (binary garbage).
        let garbage = dir.join(WAL_DIR).join("garbage.wal");
        fs::write(&garbage, b"\x00\x01\xff\xfe\x00").unwrap();

        let records = replay_wal(&dir).expect("replay must not fail");
        assert_eq!(records.len(), 1, "only the good record survives");
        assert!(!bad.exists(), "corrupt WAL file removed");
        assert!(!garbage.exists(), "garbage WAL file removed");

        // The good file is untouched.
        assert!(good.exists(), "valid WAL file preserved");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_stale_wal_removes_only_old_files() {
        let dir = tmp_dir("prune");
        let _ = fs::remove_dir_all(&dir);
        let fresh = write_wal(&dir, &[wal_record("fresh")]).expect("fresh wal");

        // A stale file far older than WAL_GRACE_SECS.
        let stale = dir.join(WAL_DIR).join("old.wal");
        fs::write(&stale, "{}").unwrap();
        let old = fs::File::options().write(true).open(&stale).unwrap();
        let ten_min_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        old.set_modified(ten_min_ago).unwrap();
        drop(old);

        prune_stale_wal(&dir);
        assert!(!stale.exists(), "stale WAL pruned");
        assert!(fresh.exists(), "fresh WAL preserved");
        let _ = fs::remove_dir_all(&dir);
    }

    // ── Cold-cache init race ────────────────────────────────────────────
    //
    // Background: `mire test -j N` gives every worker thread its own
    // IncrementalCache but they all share one cache directory. On a cold cache
    // each of them reaches `load_with_settings` at once, finds no version file,
    // and wants to wipe. Two defects used to make that unsafe, and the result in
    // CI was a modular test run reporting 8 failures, the visible symptom being
    //
    //     Cannot write cache file '<cache>/blobs/<hash>': No such file or directory
    //
    // on tests/Builtins/scalars.mire:
    //
    //  1. The index/blobs/wal directories were created *before* the init lock
    //     was taken, so a wipe could delete one a sibling had just created and
    //     that sibling then wrote into a directory that no longer existed.
    //  2. The init lock lived *inside* the cache directory, so the wipe deleted
    //     the very lock meant to prevent it. The holder kept thinking it held
    //     the lock while a second thread created the lock directory again and
    //     began a second wipe.

    /// Fixture root for the cache tests: `<repo>/tests/cache`, never /tmp, so
    /// a failure leaves its evidence where the test suite lives. Everything in
    /// there is ignored by git (see tests/cache/.gitignore).
    fn cache_fixture(tag: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("cache")
            .join(format!("{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("cache fixture dir");
        root
    }

    #[test]
    fn init_lock_survives_the_wipe_it_guards() {
        // Minimal, deterministic reproduction of defect 2 — no threads needed.
        //
        // A holder takes the init lock and then initialises, which may wipe the
        // cache. If the wipe can take the lock with it, the lock guards nothing:
        // the holder is still running, believes it is exclusive, and yet any
        // other thread can create the lock directory and start a second wipe.
        let root = cache_fixture("lock_survives_wipe");
        let cache_dir = root.join("bin").join(CACHE_DIR_NAME);
        fs::create_dir_all(&cache_dir).expect("cache dir");
        let lock = init_lock_path(&cache_dir);

        // Holder acquires the lock.
        assert!(
            fs::create_dir(&lock).is_ok(),
            "first holder must acquire the init lock"
        );

        // Holder initialises, which wipes an unversioned cache.
        assert!(
            !cache_version_is_current(&cache_dir),
            "a fresh cache must not look initialised"
        );
        wipe_cache_dir(&cache_dir);

        assert!(
            lock.exists(),
            "the wipe destroyed the init lock it is meant to guard: \
             a second thread can now wipe the same cache concurrently"
        );
        assert!(
            fs::create_dir(&lock).is_err(),
            "a second thread must not be able to acquire the init lock \
             while the first holder still holds it"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn init_rebuilds_directories_after_a_wipe() {
        // Minimal, deterministic reproduction of defect 1: the directory
        // structure must exist once `init_cache` returns, even though the
        // version did not match and triggered a wipe.
        let root = cache_fixture("rebuild_dirs");
        let cache_dir = root.join("bin").join(CACHE_DIR_NAME);
        fs::create_dir_all(&cache_dir).expect("cache dir");

        // `init_cache` wipes, so it is only ever called under the lock. Taking
        // it here keeps the test honest about how it is reached in production,
        // and lets the debug_assert inside `init_cache` stay meaningful.
        let lock = init_lock_path(&cache_dir);
        let guard = InitLockGuard(&lock);
        fs::create_dir(&lock).expect("init lock");
        init_cache(&cache_dir);
        drop(guard);

        assert!(cache_version_is_current(&cache_dir));
        for sub in [
            cache_dir.join(BLOBS_DIR),
            cache_dir.join(WAL_DIR),
            cache_dir.join(INDEX_DIR).join(FILES_INDEX),
            cache_dir.join(INDEX_DIR).join(ANALYSES_INDEX),
            cache_dir.join(INDEX_DIR).join(BUILDS_INDEX),
            cache_dir.join(INDEX_DIR).join(MIR_INDEX),
        ] {
            assert!(sub.is_dir(), "{} missing after init", sub.display());
        }

        // And a store into the rebuilt structure must not hit ENOENT, which is
        // exactly what the CI failure was.
        let hash = store_blob(&cache_dir, b"hello").expect("blob store after init");
        assert_eq!(read_blob(&cache_dir, &hash, true), Some(b"hello".to_vec()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn waiter_steals_a_stale_lock_instead_of_returning_uninitialised() {
        // Defect 2, second half: the old waiter returned as soon as the lock
        // directory disappeared, without checking that the cache had actually
        // been versioned. A holder that died between its wipe and its version
        // write left the lock gone and the cache unversioned, so the waiter
        // walked into a cache the next arriving thread would wipe under it.
        let root = cache_fixture("waiter_steals_stale");
        let cache_dir = root.join("bin").join(CACHE_DIR_NAME);
        fs::create_dir_all(&cache_dir).expect("cache dir");
        let lock = init_lock_path(&cache_dir);

        // Simulate a holder that took the lock and then died: the lock is
        // present, the cache was never versioned.
        fs::create_dir(&lock).expect("stale lock");
        assert!(!cache_version_is_current(&cache_dir));

        // With a zero staleness threshold the lock counts as abandoned, so the
        // waiter must break it and initialise the cache itself.
        wait_for_cache_init_with(
            &cache_dir,
            &lock,
            std::time::Duration::from_millis(0),
            std::time::Duration::from_secs(5),
        );

        assert!(
            cache_version_is_current(&cache_dir),
            "the waiter returned without initialising the cache"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn give_up_path_still_initialises_under_the_lock() {
        // Forces the last-resort branch: `stale_after` is so large the staleness
        // check can never fire, so the waiter can only escape via the give-up
        // deadline. The cache must end up initialised — and initialised while
        // the lock is genuinely held, which the `debug_assert!` inside
        // `init_cache` enforces for this build. An earlier version of this code
        // broke the lock and called `init_cache` directly, which passed every
        // test that checked only the outcome and reintroduced the race.
        let root = cache_fixture("give_up_holds_lock");
        let cache_dir = root.join("bin").join(CACHE_DIR_NAME);
        fs::create_dir_all(&cache_dir).expect("cache dir");
        let lock = init_lock_path(&cache_dir);

        // A holder whose age the waiter cannot read.
        fs::create_dir(&lock).expect("unreadable-age lock");
        assert!(!cache_version_is_current(&cache_dir));

        wait_for_cache_init_with(
            &cache_dir,
            &lock,
            std::time::Duration::from_secs(86_400),
            std::time::Duration::from_millis(0),
        );

        assert!(
            cache_version_is_current(&cache_dir),
            "the give-up path returned without initialising the cache"
        );
        assert!(
            !lock.exists(),
            "the waiter must release the lock it re-acquired on the give-up path"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn waiter_returns_immediately_when_the_cache_is_already_versioned() {
        // The common case: 7 of 8 threads arrive at an already-initialised
        // cache. They must not block, and must not wipe anything.
        let root = cache_fixture("waiter_fast_path");
        let cache_dir = root.join("bin").join(CACHE_DIR_NAME);
        fs::create_dir_all(&cache_dir).expect("cache dir");
        let lock = init_lock_path(&cache_dir);
        let guard = InitLockGuard(&lock);
        fs::create_dir(&lock).expect("init lock");
        init_cache(&cache_dir);
        drop(guard);

        // A marker that a stray wipe would destroy.
        let marker = cache_dir.join(BLOBS_DIR).join("marker");
        fs::write(&marker, b"kept").expect("marker");

        let started = std::time::Instant::now();
        wait_for_cache_init_with(
            &cache_dir,
            &lock,
            std::time::Duration::from_secs(30),
            std::time::Duration::from_secs(30),
        );

        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "an already-versioned cache must not make a waiter wait"
        );
        assert_eq!(fs::read(&marker).ok(), Some(b"kept".to_vec()));

        let _ = fs::remove_dir_all(&root);
    }
}
