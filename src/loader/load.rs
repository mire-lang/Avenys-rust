//! Regular `load` / `use` resolution.
//!
//! Handles resolving `load foo::bar::baz` declarations to concrete file paths
//! by walking package manifests, entry points, and export maps. Also provides
//! reachable-import inference that auto-selects only the exports the caller
//! actually uses.

use super::files::load_or_parse_file;
use super::{ImportResolver, PackageEntry};
use crate::avens::{
    EntryContainment, MireDependency, check_entry_containment, load_exports, load_project_manifest,
    resolve_export_path,
};
use crate::canonical_fn_name;
use crate::error::Result;
use crate::parser::ast::Statement;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::Span;

/// Path to the `~/.owl/libs` directory (or `$MIRE_OWL_HOME`).
pub(crate) fn owl_home_libs() -> PathBuf {
    if let Some(home) = std::env::var_os("MIRE_OWL_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    PathBuf::from(home).join(".owl").join("libs")
}

/// Extra fallback directories from repeated/colon-separated `--lib-dir`
/// values supplied by Owl. The compiler never discovers these directories by
/// itself; Owl resolves and installs the packages before invoking it.
pub(super) fn lib_dir_fallbacks() -> Vec<PathBuf> {
    std::env::var("MIRE_LIB_DIR")
        .ok()
        .map(|paths| {
            paths
                .split(':')
                .filter(|path| !path.is_empty())
                .map(expand_tilde)
                .collect()
        })
        .unwrap_or_default()
}

/// Expand a leading `~` in a path to the user's home directory.
pub(crate) fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    PathBuf::from(path)
}

/// Resolve a declared dependency (from `[dependencies]`) to its package root.
/// Mirrors the dependency branch of `resolve_package` without needing the full
/// `ImportResolver` machinery. Returns `None` if the root does not exist.
pub(crate) fn resolve_dependency_root(
    project_root: &Path,
    name: &str,
    dep: &MireDependency,
) -> Option<PathBuf> {
    let root = match dep {
        MireDependency::PathOnly { path } | MireDependency::WithPath { path, .. } => {
            let expanded = expand_tilde(path);
            if expanded.is_absolute() {
                expanded
            } else {
                project_root.join(expanded)
            }
        }
        MireDependency::Simple { .. } => owl_home_libs().join(name),
    };
    if root.exists() { Some(root) } else { None }
}

/// Resolve a package name to its `(root_path, entry_string)`.
///
/// Checks the in-process path cache first, then uses the explicit library
/// directory supplied by Owl (`--lib-dir`/`MIRE_LIB_DIR`). Avenys deliberately
/// does not read the consumer project's dependency table or contact a
/// registry; dependency selection and installation belong to Owl.
pub(super) fn resolve_package(
    resolver: &mut ImportResolver,
    name: &str,
    span: Span,
) -> Result<(PathBuf, String)> {
    if let Some(entry) = resolver.package_registry.get(name) {
        return Ok((entry.root.clone(), entry.entry.clone()));
    }
    let fallback_dirs = lib_dir_fallbacks();
    let package_root = if let Some(fallback_path) = fallback_dirs
        .iter()
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.exists())
    {
        fallback_path
    } else {
        return Err(resolver.loader_error(
            span,
            format!(
                "Package '{}' is not installed in the library directories supplied by Owl",
                name,
            ),
        ));
    };

    let canonical_root = package_root.canonicalize().map_err(|err| {
        resolver.loader_error(
            span,
            format!(
                "Could not resolve package '{}' at '{}': {}",
                name,
                package_root.display(),
                err
            ),
        )
    })?;

    let manifest = load_project_manifest(&canonical_root)?;
    let entry = manifest
        .as_ref()
        .map(|m| m.project.entry.clone())
        .unwrap_or_else(|| {
            if canonical_root.join("mod.mire").exists() {
                "mod.mire".to_string()
            } else {
                "mod.mr".to_string()
            }
        });

    // Path containment (docs/SECURITY.md item 5): a manifest entry that is
    // absolute or resolves outside the package root must not be loaded.
    if check_entry_containment(&canonical_root, &entry) == EntryContainment::EscapesRoot {
        return Err(resolver.loader_error(
            span,
            format!(
                "Package '{}' entry '{}' escapes the package root",
                name, entry
            ),
        ));
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
                resolver.loader_error(
                    span,
                    format!("Package '{}' has no export '{}'", segments[0], segment),
                )
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
            return Err(resolver.loader_error(
                span,
                format!(
                    "Cannot resolve '{}': '{}' has no sub-exports",
                    segments[i + 1..].join("::"),
                    segment
                ),
            ));
        }
    }

    unreachable!()
}

/// When `ImportMode::Reachable` is active and no explicit items are given,
/// parse the target file and select only exports matching the caller's
/// dependency candidates.
///
/// `module_segments` is the load path that reached this file (for example
/// `["math", "float"]` for `load kioto::math::float`). The statements this
/// module contributes are named after the *last* segment (`float.sign`), but a
/// consumer spells them with the namespace it wrote in its own source
/// (`math::float::sign`), and the candidate set is built from that spelling.
/// Matching on the bare export name alone therefore misses every qualified
/// call: `sign` does not equal `math.float.sign`, and the only reason such a
/// module kept any symbols at all was an unrelated tail collision elsewhere in
/// the candidate set. So an export is selected when a candidate *ends with* the
/// export, with or without the module's own namespace in front.
pub(super) fn infer_reachable_import_items(
    resolver: &mut ImportResolver,
    path: &Path,
    module_segments: &[String],
    candidates: &HashSet<String>,
) -> Result<Option<Vec<String>>> {
    let parsed = load_or_parse_file(resolver, path, None)?;
    if parsed.exports.is_empty() {
        return Ok(None);
    }

    // `extern lib` and `module` statements are not selectable exports — they
    // are pulled in transitively when a real export's dependency chain is
    // resolved. Treating them as exports lets a bare lib name (e.g. "SDL3"
    // from `extern fn ... lib "SDL3"`) match the candidate set and select
    // only the extern-lib statement, dropping the module's actual functions.
    let mut non_selectable = HashSet::new();
    let mut namespace_exports = HashSet::new();
    for statement in &parsed.program.statements {
        match statement {
            Statement::ExternLib { name, .. } | Statement::Module { name } => {
                non_selectable.insert(name.clone());
            }
            Statement::Load { path, alias, .. } => {
                let name = alias
                    .as_deref()
                    .unwrap_or_else(|| path.last().map(String::as_str).unwrap_or(""));
                if !name.is_empty() {
                    namespace_exports.insert(name.to_string());
                }
            }
            _ => {}
        }
    }

    let mut selected = Vec::new();
    for export in &parsed.exports {
        if non_selectable.contains(export) {
            continue;
        }
        if candidate_reaches_export(candidates, module_segments, export) {
            selected.push(export.to_string());
            continue;
        }
        // A sub-module Load statement is a namespace export. When a caller
        // references a symbol under that namespace (`timer.delay_precise` →
        // export `timer`), the whole namespace tree must be pulled in: nested
        // sub-modules keep their own independent prefixes (e.g. kioto's
        // `complex.new`, not `math.complex.new`), so a per-statement selection
        // on the `math.` prefix would drop them. Bail to a full load instead.
        if namespace_exports.contains(export)
            && candidates
                .iter()
                .any(|candidate| candidate_references_namespace(candidate, export))
        {
            return Ok(None);
        }
    }

    // A private `extern fn` is a declaration of a foreign symbol, not an
    // implementation, and a consumer is allowed to name it even though the
    // library did not publish it: `sdl3`'s `events` module declares
    // `rt_write_i32` and sdl's own test calls it directly. `parsed.exports` only
    // lists `pub` statements, so reachable selection dropped the declaration
    // and the consumer failed with `Unknown function 'rt_write_i32'` — a symbol
    // the library really does declare.
    //
    // A reference to one of those makes this module's public surface the wrong
    // thing to select against, so it bails to a full load exactly as the
    // namespace case above does. Adding the extern to the item list instead
    // would be wrong twice over: it would leave the module's real exports
    // behind, and in a module whose only candidate is the extern it would turn
    // what used to be a full load into a one-item selection.
    for statement in &parsed.program.statements {
        if let Statement::ExternFunction {
            name,
            visibility: crate::parser::ast::Visibility::Private,
            ..
        } = statement
            && candidate_reaches_export(candidates, module_segments, name)
        {
            return Ok(None);
        }
    }

    if selected.is_empty() {
        return Ok(None);
    }
    selected.sort();
    selected.dedup();
    Ok(Some(selected))
}

/// True when some candidate names this export, either bare or under the
/// module's own namespace.
fn candidate_reaches_export(
    candidates: &HashSet<String>,
    module_segments: &[String],
    export: &str,
) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate_reaches_export_namespaced(candidate, module_segments, export))
}

/// True when `candidate` names `export`, bare or namespaced.
///
/// A candidate matches when it equals the export or ends with `.{export}`, and
/// additionally when it ends with `.{namespace}.{export}` for the namespace the
/// module was loaded under. The suffix form is what makes `math.float.sign`
/// select `sign` in a module reached as `kioto::math::float`; the leading
/// segments belong to whichever enclosing package spelled the call and are not
/// knowable from inside this module.
fn candidate_reaches_export_namespaced(
    candidate: &str,
    module_segments: &[String],
    export: &str,
) -> bool {
    let candidate = canonical_fn_name(candidate);
    let export_tail = canonical_fn_name(export);
    if candidate == export_tail || candidate.ends_with(&format!(".{export_tail}")) {
        return true;
    }
    let namespace = module_segments
        .iter()
        .map(|segment| canonical_fn_name(segment))
        .collect::<Vec<_>>()
        .join(".");
    if namespace.is_empty() {
        return false;
    }
    let qualified = format!("{namespace}.{export_tail}");
    candidate == qualified || candidate.ends_with(&format!(".{qualified}"))
}

/// True when `candidate` references something *inside* the `namespace` named
/// by `export`.
///
/// This is deliberately a segment test rather than a prefix test. A
/// sub-module's namespace can appear anywhere in a consumer's spelling: the
/// module reached as `kioto::math` exports the namespace `float`, and a
/// consumer may write `float::sign` (namespace first) or `math::float::sign`
/// (one enclosing namespace in front). Only a `starts_with` test recognises
/// the first form, so with it the second form silently selected just the root
/// module's own `sign` and dropped the entire `float` sub-tree.
///
/// Matching on whole segments — and requiring the namespace to be a
/// non-final segment, since a candidate that merely *is* the namespace names
/// the module rather than something inside it — accepts both forms while still
/// rejecting `floaty::sign` for the `float` namespace.
fn candidate_references_namespace(candidate: &str, namespace: &str) -> bool {
    let candidate = canonical_fn_name(candidate);
    let namespace = canonical_fn_name(namespace);
    if namespace.is_empty() {
        return false;
    }
    let segments: Vec<&str> = candidate.split('.').collect();
    segments
        .windows(2)
        .any(|pair| pair[0] == namespace.as_str())
}
