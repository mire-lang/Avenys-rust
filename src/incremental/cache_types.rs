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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileMeta {
    pub(super) hash: u64,
    pub(super) hash2: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AnalysisMeta {
    pub(super) fingerprint: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
    pub(super) created_ms: u64,
    pub(super) unit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BuildMeta {
    pub(super) fingerprint: u64,
    pub(super) entry: BuildCacheEntry,
    pub(super) last_access_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MirMeta {
    pub(super) body_hash: u64,
    pub(super) blob_hash: String,
    pub(super) last_access_ms: u64,
}
