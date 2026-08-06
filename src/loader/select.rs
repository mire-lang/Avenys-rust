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
/// If `items` is `None`, returns only public exports and impl blocks.
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
                    statement_export_name(&statement.1.statement)
                        .is_some_and(|name| name == item.as_str() || name.starts_with(&format!("{item}.")))
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

        let mut reachable = Vec::new();
        for (idx, statement) in statements.iter().enumerate() {
            if selected.contains(&idx) {
                reachable.push(statement.clone());
            }
        }
        return Ok(reachable);
    }

    let result: Vec<ExpandedStatement> = statements
        .iter()
        .filter(|statement| {
            statement_export_name(&statement.statement).is_some()
                || matches!(&statement.statement, Statement::Impl { .. })
        })
        .cloned()
        .collect();
    Ok(result)
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
                    Statement::ExternFunction { name, .. }
                    | Statement::ExternLib { name, .. } => Some(name.as_str()),
                    Statement::Let { name, .. } => Some(name.as_str()),
                    Statement::Assignment {
                        target: AssignmentTarget::Variable(name),
                        ..
                    } => Some(name.as_str()),
                    _ => None,
                };
                // A namespace-parent export (e.g. `vec.push`) satisfies a
                // dependency on one of its children (e.g. `vec.push.i64`)
                // because the child lives nested in the parent's body.
                let namespace_parent_match = normalized_export.as_deref().is_some_and(|name| {
                    normalized_dep.starts_with(name)
                        && normalized_dep.as_bytes().get(name.len()) == Some(&b'.')
                });
                if (normalized_export.as_deref() == Some(candidate_name)
                    || internal_name == Some(candidate_name)
                    || namespace_parent_match)
                    && selected.insert(dep_idx)
                {
                    selected_indices.push(dep_idx);
                }
            }
        }
    }
}
