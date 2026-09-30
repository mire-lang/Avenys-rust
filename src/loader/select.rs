//! Import selection and transitive dependency resolution.
//!
//! After loading a module's statements, this module decides which of those
//! statements are actually needed by the caller. It performs transitive
//! dependency resolution: for each requested import, it walks
//! `collect_statement_dependencies` and includes any statements that
//! provide those dependencies. It also includes `Impl` blocks for
//! selected types.

use super::ExpandedStatement;
use crate::canonical_fn_name;
use crate::error::{ErrorKind, MireError, Result};
use crate::incremental::{collect_statement_dependencies, statement_export_name};
use crate::parser::ast::{AssignmentTarget, Statement};
use std::collections::HashSet;
use std::path::Path;

use crate::error::Span;

/// Filter an expanded statement list to only the named `items`.
///
/// If `items` is `Some`, performs transitive dependency resolution:
/// for each selected item, walks its `collect_statement_dependencies`
/// and includes any statements that provide those dependencies. Also
/// includes `Impl` blocks for selected types.
///
/// If `items` is `None`, returns the public exports and impl blocks, plus
/// everything they depend on (private helpers in the same module, module-level
/// mutable globals, module constants) so the retained exports still resolve.
pub(super) fn select_imported_statements(
    statements: &[ExpandedStatement],
    items: Option<&[String]>,
    import_path: &Path,
    span: Span,
) -> Result<Vec<ExpandedStatement>> {
    if let Some(items) = items {
        let mut selected_indices = Vec::new();
        let mut selected = HashSet::new();
        for item in items {
            // Exact export match, or namespace-prefix match when `item` refers
            // to a sub-module Load statement (e.g. `timer` → `timer.delay_precise`).
            let matched: Vec<usize> = statements
                .iter()
                .enumerate()
                .filter(|statement| {
                    statement_export_name(&statement.1.statement).is_some_and(|name| {
                        name == item.as_str() || name.starts_with(&format!("{item}."))
                    })
                })
                .map(|(idx, _)| idx)
                .collect();
            if matched.is_empty() {
                return Err(MireError::new(ErrorKind::Runtime {
                    span,
                    message: format!(
                        "Local load '{}' does not export '{}'",
                        import_path.display(),
                        item
                    ),
                }));
            }
            for idx in matched {
                if selected.insert(idx) {
                    selected_indices.push(idx);
                }
            }
        }

        let mut cursor = 0usize;
        while cursor < selected_indices.len() {
            let idx = selected_indices[cursor];
            cursor += 1;

            resolve_statement_deps(
                &statements[idx].statement,
                statements,
                &mut selected,
                &mut selected_indices,
            );
        }

        // Second pass: include impl blocks for selected types, then process their deps
        let mut selected_types: HashSet<String> = HashSet::new();
        for idx in &selected_indices {
            if let Statement::Type { name, .. } | Statement::Enum { name, .. } =
                &statements[*idx].statement
            {
                selected_types.insert(name.clone());
            }
        }
        for (idx, statement) in statements.iter().enumerate() {
            if !selected.contains(&idx)
                && let Statement::Impl { type_name, .. } = &statement.statement
            {
                let base = type_name.rsplit('.').next().unwrap_or(type_name);
                if selected_types.contains(type_name) || selected_types.contains(base) {
                    selected.insert(idx);
                    selected_indices.push(idx);
                }
            }
        }
        // Process dependencies of newly added impl blocks (they reference trait names)
        while cursor < selected_indices.len() {
            let idx = selected_indices[cursor];
            cursor += 1;

            resolve_statement_deps(
                &statements[idx].statement,
                statements,
                &mut selected,
                &mut selected_indices,
            );
        }

        // Module-level constants (`cons`) are part of a module's implementation:
        // exported functions may reference them even though the constants
        // themselves are not exported. They must always travel with the module
        // so a consumer that selects a subset of the module's functions still
        // gets every constant those functions read.
        let mut reachable = Vec::new();
        for (idx, statement) in statements.iter().enumerate() {
            if selected.contains(&idx) || is_module_constant(&statement.statement) {
                reachable.push(statement.clone());
            }
        }
        return Ok(reachable);
    }

    // No explicit item list: the whole public surface travels, and so does
    // everything those exports need in order to resolve. A `pub fn` is free to
    // call a private `fn` in the same module, and dropping the callee leaves a
    // reference the backend cannot resolve — which surfaces as an opaque
    // "Unknown function 'helper'" at the call site. Walking the dependency
    // closure here keeps private helpers, module-level mutable `set` globals
    // and any other non-exported statement that a retained export actually
    // reads, while still leaving unreferenced internals behind.
    let mut selected: HashSet<usize> = HashSet::new();
    let mut selected_indices: Vec<usize> = Vec::new();
    for (idx, statement) in statements.iter().enumerate() {
        if statement_export_name(&statement.statement).is_some()
            || matches!(&statement.statement, Statement::Impl { .. })
            || is_module_constant(&statement.statement)
        {
            selected.insert(idx);
            selected_indices.push(idx);
        }
    }

    let mut cursor = 0usize;
    while cursor < selected_indices.len() {
        let idx = selected_indices[cursor];
        cursor += 1;

        resolve_statement_deps(
            &statements[idx].statement,
            statements,
            &mut selected,
            &mut selected_indices,
        );
    }

    // Emit in source order regardless of the order the closure discovered
    // them, so the result does not depend on which export happened to be
    // visited first.
    let mut ordered: Vec<usize> = selected_indices;
    ordered.sort_unstable();

    let result: Vec<ExpandedStatement> = ordered
        .into_iter()
        .map(|idx| statements[idx].clone())
        .collect();
    Ok(result)
}

/// A module-level constant (`cons`). These are retained alongside any module
/// selection so exported functions that read them always resolve.
fn is_module_constant(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Let {
            is_constant: true,
            ..
        }
    )
}

/// Resolve transitive dependencies of a single statement, adding any
/// matching statements from the full list to the selected set.
fn resolve_statement_deps(
    statement: &Statement,
    all_statements: &[ExpandedStatement],
    selected: &mut HashSet<usize>,
    selected_indices: &mut Vec<usize>,
) {
    let mut deps = Vec::new();
    collect_statement_dependencies(statement, &mut deps);
    for dependency in deps {
        let normalized_dep = canonical_fn_name(&dependency);
        for candidate in [
            Some(normalized_dep.as_str()),
            normalized_dep.rsplit_once('.').map(|(_, tail)| tail),
        ] {
            let Some(candidate_name) = candidate else {
                continue;
            };
            for (dep_idx, stmt) in all_statements.iter().enumerate() {
                let export_name = statement_export_name(&stmt.statement);
                let normalized_export = export_name.map(canonical_fn_name);
                let internal_name = match &stmt.statement {
                    Statement::ExternFunction { name, .. } | Statement::ExternLib { name, .. } => {
                        Some(name.as_str())
                    }
                    // A private function has no export name at all, so this is
                    // the only handle the selection has on it.
                    Statement::Function { name, .. } => Some(name.as_str()),
                    Statement::Let { name, .. } => Some(name.as_str()),
                    Statement::Assignment {
                        target: AssignmentTarget::Variable(name),
                        ..
                    } => Some(name.as_str()),
                    _ => None,
                };
                let internal_name = internal_name.map(canonical_fn_name);
                // A dependency is recorded with the callee spelled the way the
                // call site wrote it, but a definition inside a prefixed module
                // is stored with that prefix already applied: a private
                // `magnitude` helper in `math::int` lives at `int.magnitude`
                // while its caller records `magnitude`. Both sides therefore
                // have to be compared whole and by tail.
                let whole_match = |name: Option<&str>| name == Some(candidate_name);
                // The tail comparison is confined to non-exported definitions.
                // Those are the ones with no public path to match on, and
                // keeping it narrow leaves the exported surface, where a short
                // name like `push` can legitimately appear in several
                // namespaces, matching exactly as before.
                let tail_match = |name: Option<&str>| {
                    name.is_some_and(|name| {
                        normalized_export.is_none()
                            && name.rsplit_once('.').is_some_and(|(_, tail)| tail == candidate_name)
                    })
                };
                // A namespace-parent export (e.g. `vec.push`) satisfies a
                // dependency on one of its children (e.g. `vec.push.i64`)
                // because the child lives nested in the parent's body.
                let namespace_parent_match = normalized_export.as_deref().is_some_and(|name| {
                    normalized_dep.starts_with(name)
                        && normalized_dep.as_bytes().get(name.len()) == Some(&b'.')
                });
                if (whole_match(normalized_export.as_deref())
                    || whole_match(internal_name.as_deref())
                    || tail_match(internal_name.as_deref())
                    || namespace_parent_match)
                    && selected.insert(dep_idx)
                {
                    selected_indices.push(dep_idx);
                }
            }
        }
    }
}
