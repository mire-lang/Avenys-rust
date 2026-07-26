//! File I/O, parsing, and incremental cache integration.
//!
//! Handles reading source files from disk, parsing them into ASTs,
//! and managing the incremental cache for fast recompilation.

use super::{ImportResolver, ResolvedFile};
use crate::error::{MireError, Result};
use crate::incremental::{
    CachedParsedFile, collect_statement_bindings, collect_statement_dependencies,
    source_hash, source_hash2, statement_export_name,
};
use crate::parser::{Program, parse};
use crate::parser::ast::Statement;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Read a source file from disk into a `String`.
pub(super) fn read_source_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| {
        MireError::new(crate::error::ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not read '{}': {}", path.display(), err),
        })
    })
}

/// Load or parse a file, using the incremental cache when possible.
pub(super) fn load_or_parse_file(
    resolver: &mut ImportResolver,
    path: &Path,
) -> Result<ResolvedFile> {
    let source = read_source_file(path)?;
    resolver.sources.insert(path.to_path_buf(), source.clone());
    let hash = source_hash(&source);
    let hash2 = source_hash2(&source);
    if let Some(cached) = resolver.cache.cached_file(path, hash, hash2) {
        return Ok(ResolvedFile {
            hash,
            program: cached.program,
            exports: cached.exports,
        });
    }

    let program = parse(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    let exports: Vec<String> = program
        .statements
        .iter()
        .filter_map(statement_export_name)
        .map(ToString::to_string)
        .collect();
    resolver.cache.store_file(
        path,
        CachedParsedFile {
            hash,
            hash2,
            exports: exports.clone(),
            local_imports: Vec::new(),
            program: program.clone(),
        },
    )?;
    Ok(ResolvedFile {
        hash,
        program,
        exports,
    })
}

/// Scan a program's statements to collect all identifier dependency candidates.
///
/// These are used by the reachable-import inference to decide which exports
/// from a loaded module are actually needed by the caller.
pub(super) fn collect_program_dependency_candidates(program: &Program) -> HashSet<String> {
    let mut candidates = HashSet::new();
    let mut local_bindings = HashSet::new();
    for statement in &program.statements {
        if matches!(statement, Statement::Load { .. }) {
            continue;
        }
        let mut deps = Vec::new();
        collect_statement_dependencies(statement, &mut deps);
        for dep in deps {
            candidates.insert(dep.clone());
            if let Some((_, tail)) = dep.rsplit_once('.') {
                candidates.insert(tail.to_string());
            }
            if let Some((_, tail)) = dep.rsplit_once("::") {
                candidates.insert(tail.to_string());
            }
        }
        let mut bindings = Vec::new();
        collect_statement_bindings(statement, &mut bindings);
        for b in bindings {
            local_bindings.insert(b);
        }
    }
    // Remove local variable names that would otherwise falsely match
    // external module exports (e.g. parameter name "min" matching a
    // function export "min" from another module).
    candidates.retain(|c| !local_bindings.contains(c));
    candidates
}
