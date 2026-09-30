//! Local `load!` / `use!` resolution.
//!
//! Handles resolving `load! path::to::module` declarations that refer to
//! project-local files rather than external packages. Resolves paths
//! relative to the project root (absolute) or the current file's directory
//! (relative), with a protected depth and graph limit in the resolver.

use super::ImportResolver;
use crate::error::{Result, Span};
use std::path::{Path, PathBuf};

/// Resolve a `load!` (LoadLocal) target.
///
/// Tries three candidates in order:
/// 1. `<dir>/main.mire` or `<dir>/main.mr`
/// 2. `<dir>/mod.mire` or `<dir>/mod.mr`
/// 3. `<dir>.mire` or `<dir>.mr`
///
/// Returns the resolved file path and the directory depth below the project
/// root (used for diagnostics; the safety budget is enforced by `load_file`).
pub(super) fn resolve_load_local_target(
    resolver: &ImportResolver,
    rel_path: &[String],
    absolute: bool,
    current_dir: &Path,
    span: Span,
) -> Result<(PathBuf, usize)> {
    let base = if absolute {
        resolver.project_root.clone()
    } else {
        current_dir.to_path_buf()
    };
    let joined: PathBuf = rel_path.iter().collect();
    let candidate_dir = base.join(&joined);
    let candidates = [
        candidate_dir.join("main.mire"),
        candidate_dir.join("main.mr"),
        candidate_dir.join("mod.mire"),
        candidate_dir.join("mod.mr"),
        candidate_dir.with_extension("mire"),
        candidate_dir.with_extension("mr"),
    ];
    let target = candidates.into_iter().find(|p| p.exists()).ok_or_else(|| {
        resolver.loader_error(
            span,
            format!("load! target '{}' not found", rel_path.join("/")),
        )
    })?;
    let depth = target
        .parent()
        .and_then(|p| p.strip_prefix(&resolver.project_root).ok())
        .map(|rel| {
            rel.components()
                .filter(|c| {
                    !matches!(
                        c,
                        std::path::Component::CurDir | std::path::Component::ParentDir
                    )
                })
                .count()
        })
        .unwrap_or(0);
    Ok((target, depth))
}
