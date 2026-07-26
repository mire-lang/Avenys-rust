//! Regular `load` / `use` resolution.
//!
//! Handles resolving `load foo::bar::baz` declarations to concrete file paths
//! by walking package manifests, entry points, and export maps. Also provides
//! reachable-import inference that auto-selects only the exports the caller
//! actually uses.

use super::{ImportResolver, PackageEntry};
use super::files::load_or_parse_file;
use crate::avens::{load_exports, load_project_manifest, resolve_export_path};
use crate::canonical_fn_name;
use crate::error::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::Span;

/// Path to the `~/.owl/libs` directory (or `$MIRE_OWL_HOME`).
pub(super) fn owl_home_libs() -> PathBuf {
    if let Some(home) = std::env::var_os("MIRE_OWL_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    PathBuf::from(home).join(".owl").join("libs")
}

/// Resolve a package name to its `(root_path, entry_string)`.
///
/// Checks the package registry cache first, then consults manifest
/// dependencies (path-only, with-path, or simple home-path). Special-cases
/// `"kioto"` for backward compatibility.
pub(super) fn resolve_package(
    resolver: &mut ImportResolver,
    name: &str,
    span: Span,
) -> Result<(PathBuf, String)> {
    if let Some(entry) = resolver.package_registry.get(name) {
        return Ok((entry.root.clone(), entry.entry.clone()));
    }
    let package_root = if let Some(dep) = resolver.manifest_dependencies.get(name) {
        match dep {
            crate::avens::MireDependency::PathOnly { path }
            | crate::avens::MireDependency::WithPath { path, .. } => {
                let p = PathBuf::from(path);
                if p.is_absolute() {
                    p
                } else {
                    resolver.project_root.join(p)
                }
            }
            crate::avens::MireDependency::Simple { .. } => owl_home_libs().join(name),
        }
    } else if name == "kioto" {
        let home_path = owl_home_libs().join("kioto");
        if home_path.exists() {
            home_path
        } else {
            let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let dev_path = crate_dir.join("../kioto");
            if dev_path.exists() {
                dev_path
            } else {
                resolver.project_root.join("../kioto")
            }
        }
    } else {
        return Err(resolver.loader_error(span, format!(
            "Package '{}' not found in [dependencies] of {}",
            name,
            resolver.project_root.join("owl.toml").display()
        )));
    };

    let canonical_root = package_root.canonicalize().map_err(|err| {
        resolver.loader_error(span, format!(
            "Could not resolve package '{}' at '{}': {}",
            name,
            package_root.display(),
            err
        ))
    })?;

    let manifest = load_project_manifest(&canonical_root)?;
    let entry = manifest
        .as_ref()
        .map(|m| m.project.entry.clone())
        .unwrap_or_else(|| "mod.mire".to_string());

    if let Some(ref m) = manifest {
        for (dep_name, dep) in &m.dependencies.entries {
            resolver
                .manifest_dependencies
                .entry(dep_name.clone())
                .or_insert_with(|| dep.clone());
        }
    }

    resolver.package_registry.insert(
        name.to_string(),
        PackageEntry {
            root: canonical_root.clone(),
            entry: entry.clone(),
        },
    );

    Ok((canonical_root, entry))
}

/// Resolve a multi-segment `load` path (e.g. `load foo::bar::baz`) into a
/// concrete file path.
///
/// Calls `resolve_package` for the first segment, then walks subsequent
/// segments through `resolve_export_path` and nested package exports.
pub(super) fn resolve_load_path(
    resolver: &mut ImportResolver,
    segments: &[String],
    span: Span,
) -> Result<PathBuf> {
    let (mut current_root, entry) = resolve_package(resolver, &segments[0], span)?;
    let mut current_exports = load_exports(&current_root).unwrap_or_default();

    if segments.len() == 1 {
        let direct = current_root.join(&entry);
        if direct.exists() {
            return Ok(direct);
        }
        if let Some(export_path) =
            resolve_export_path(&current_exports, &current_root, &segments[0])
            && export_path.exists()
        {
            return Ok(export_path);
        }
        return Ok(direct);
    }

    for i in 1..segments.len() {
        let segment = &segments[i];
        let is_last = i == segments.len() - 1;

        let target =
            resolve_export_path(&current_exports, &current_root, segment).ok_or_else(|| {
                resolver.loader_error(span, format!("Package '{}' has no export '{}'", segments[0], segment))
            })?;

        if is_last {
            return Ok(target);
        }

        let parent = if target.is_dir() {
            target.clone()
        } else {
            target.parent().unwrap_or(&current_root).to_path_buf()
        };

        if parent.join("owl.toml").exists() {
            current_exports = load_exports(&parent).unwrap_or_default();
            current_root = parent;
        } else {
            return Err(resolver.loader_error(span, format!(
                "Cannot resolve '{}': '{}' has no sub-exports",
                segments[i + 1..].join("::"),
                segment
            )));
        }
    }

    unreachable!()
}

/// When `ImportMode::Reachable` is active and no explicit items are given,
/// parse the target file and select only exports matching the caller's
/// dependency candidates.
pub(super) fn infer_reachable_import_items(
    resolver: &mut ImportResolver,
    path: &Path,
    module_prefix: Option<&str>,
    candidates: &HashSet<String>,
) -> Result<Option<Vec<String>>> {
    let parsed = load_or_parse_file(resolver, path)?;
    if parsed.exports.is_empty() {
        return Ok(None);
    }

    let mut selected = Vec::new();
    for export in &parsed.exports {
        let normalized = canonical_fn_name(export);
        let export_tail = normalized
            .rsplit_once('.')
            .map_or(normalized.as_str(), |(_, tail)| tail);
        let prefixed = module_prefix.map(|prefix| format!("{prefix}.{export_tail}"));
        if candidates.contains(export)
            || candidates.contains(&normalized)
            || candidates.contains(export_tail)
            || prefixed
                .as_ref()
                .is_some_and(|value| candidates.contains(value))
        {
            selected.push(export.to_string());
        }
    }

    if selected.is_empty() {
        return Ok(None);
    }
    selected.sort();
    selected.dedup();
    Ok(Some(selected))
}
