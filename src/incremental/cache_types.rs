use super::BuildCacheEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub(super) enum WalRecord {
    StoreFile { key: String, hash: u64, hash2: u64, blob_hash: String, timestamp: u64 },
    StoreAnalysis { key: String, fingerprint: u64, blob_hash: String, timestamp: u64, created_ms: u64, unit_count: u32 },
    StoreBuild { key: String, entry: BuildCacheEntry, timestamp: u64 },
    DeleteFile { key: String, timestamp: u64 },
    DeleteAnalysis { key: String, timestamp: u64 },
    DeleteBuild { key: String, timestamp: u64 },
    StoreMirFn { key: String, body_hash: u64, blob_hash: String, timestamp: u64 },
    DeleteMirFn { key: String, timestamp: u64 },
    Checkpoint { timestamp: u64 },
}

/// Meta records persist the original cache key so that reloading the cache can
/// restore the in-memory maps keyed by the real key (not the hashed filename).
/// `#[serde(default)]` keeps old meta files (written before the `key` field
/// existed) deserializable; those fall back to the hashed filename stem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileMeta {
    #[serde(default)]
    pub(super) key: String,
    pub(super) hash: u64,
    pub(super) hash2: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AnalysisMeta {
    #[serde(default)]
    pub(super) key: String,
    pub(super) fingerprint: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
    pub(super) created_ms: u64,
    pub(super) unit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BuildMeta {
    #[serde(default)]
    pub(super) key: String,
    pub(super) fingerprint: u64,
    pub(super) entry: BuildCacheEntry,
    pub(super) last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MirMeta {
    #[serde(default)]
    pub(super) key: String,
    pub(super) body_hash: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
}
