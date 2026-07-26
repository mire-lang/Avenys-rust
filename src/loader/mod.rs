//! Module loading and import resolution.
//!
//! This module handles the `load`, `use`, `load!`, and `use!` declarations
//! in Mire programs. It resolves packages, manages the incremental cache,
//! performs module-level name prefixing, and selects only the imports that
//! are actually reachable.
//!
//! # Architecture
//!
//! - `mod.rs`  — public API, `ImportResolver` struct, `load_file` dispatcher
//! - `files.rs` — file I/O, parsing, incremental cache, dependency candidates
//! - `load.rs`  — regular `load`/`use` package resolution and path walking
//! - `lload.rs` — local `load!`/`use!` file resolution with depth limit
//! - `select.rs` — import selection and transitive dependency resolution
//! - `rename.rs` — module-renamer that prefixes loaded names with aliases

mod files;
mod load;
mod lload;
mod rename;
mod select;

use crate::avens::{ImportMode, MireDependency, find_project_root, load_manifest_dependencies};
use crate::error::{ErrorKind, MireError, Result, Span};
use crate::incremental::{
    CacheSettings, IncrementalCache, LoadedFile, LoadedProgram, statement_export_name,
};
use crate::parser::ast::{
    AssignmentTarget, DataType, EnumVariantDef, Expression, Identifier, Literal, Statement,
};
use crate::parser::Program;
use files::{collect_program_dependency_candidates, load_or_parse_file};
use rename::prefix_loaded_statements_scoped;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct PackageEntry {
    pub(crate) root: PathBuf,
    pub(crate) entry: String,
}

struct ImportResolver<'a> {
    project_root: PathBuf,
    cache: &'a mut IncrementalCache,
    expanded_cache: HashMap<PathBuf, Vec<ExpandedStatement>>,
    active_stack: HashSet<PathBuf>,
    files: HashMap<PathBuf, LoadedFile>,
    sources: HashMap<PathBuf, String>,
    import_mode: ImportMode,
    manifest_dependencies: HashMap<String, MireDependency>,
    package_registry: HashMap<String, PackageEntry>,
    current_file: Option<String>,
}

struct ResolvedFile {
    hash: u64,
    program: Program,
    exports: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ExpandedStatement {
    pub(crate) statement: Statement,
    pub(crate) origin: PathBuf,
}

// ---------------------------------------------------------------------------
// ImportResolver — core methods
// ---------------------------------------------------------------------------

impl<'a> ImportResolver<'a> {
    fn new(
        project_root: PathBuf,
        cache: &'a mut IncrementalCache,
        import_mode: ImportMode,
        manifest_dependencies: HashMap<String, MireDependency>,
    ) -> Self {
        Self {
            project_root,
            cache,
            expanded_cache: HashMap::new(),
            active_stack: HashSet::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
            import_mode,
            manifest_dependencies,
            package_registry: HashMap::new(),
            current_file: None,
        }
    }

    fn loader_error(&self, span: Span, message: String) -> MireError {
        let mut err = MireError::new(ErrorKind::Runtime { span, message });
        if let Some(ref file) = self.current_file {
            err = err.with_filename(file.clone());
        }
        err
    }

    /// Central file-loading orchestrator.
    ///
    /// Canonicalizes the path, checks the expanded cache, detects cyclic
    /// loads via `active_stack`, loads/parses the file, then iterates over
    /// statements dispatching to:
    /// - **`Statement::Load`** → `load` submodule (package resolution)
    /// - **`Statement::LoadLocal`** → `lload` submodule (local resolution)
    /// - Everything else → pass through
    fn load_file(&mut self, path: &Path, span: Span) -> Result<Vec<ExpandedStatement>> {
        let canonical = path.canonicalize().map_err(|err| {
            self.loader_error(span, format!("Could not resolve '{}': {}", path.display(), err))
        })?;

        if let Some(cached) = self.expanded_cache.get(&canonical) {
            return Ok(cached.clone());
        }

        if !self.active_stack.insert(canonical.clone()) {
            return Err(self.loader_error(span, format!("Cyclic local load detected at '{}'", canonical.display())));
        }

        let parsed = load_or_parse_file(self, &canonical)?;
        self.current_file = Some(canonical.display().to_string());
        let imported_symbol_candidates = collect_program_dependency_candidates(&parsed.program);
        let mut expanded = Vec::new();
        let mut direct_dependencies = Vec::new();
        let mut dep_set = HashSet::new();

        for statement in parsed.program.statements {
            match statement {
                Statement::Load { path, alias, items, line, column }
                    if !path.is_empty() && !path[0].starts_with("__") =>
                {
                    self.expand_load(
                        &canonical,
                        path, alias, items, line, column,
                        &imported_symbol_candidates,
                        &mut expanded,
                        &mut direct_dependencies,
                        &mut dep_set,
                    )?;
                }
                Statement::LoadLocal { rel_path, absolute, line, column } => {
                    self.expand_load_local(
                        &canonical,
                        rel_path, absolute, line, column,
                        &mut expanded,
                        &mut direct_dependencies,
                        &mut dep_set,
                    )?;
                }
                other => expanded.push(ExpandedStatement {
                    statement: other,
                    origin: canonical.clone(),
                }),
            }
        }

        self.active_stack.remove(&canonical);
        self.files.insert(
            canonical.clone(),
            LoadedFile {
                hash: parsed.hash,
                direct_dependencies,
            },
        );
        self.expanded_cache
            .insert(canonical.clone(), expanded.clone());
        Ok(expanded)
    }

    /// Handle `load` statement — delegates to `load` submodule.
    #[allow(clippy::too_many_arguments)]
    fn expand_load(
        &mut self,
        _canonical: &Path,
        path: Vec<String>,
        alias: Option<String>,
        items: Option<Vec<String>>,
        line: usize,
        column: usize,
        imported_symbol_candidates: &HashSet<String>,
        expanded: &mut Vec<ExpandedStatement>,
        direct_dependencies: &mut Vec<PathBuf>,
        dep_set: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let load_span = Span::new(line, column);
        let target = match load::resolve_load_path(self, &path, load_span) {
            Ok(target) => target,
            // Fallback for3+ segment paths: try loading the parent module
            // and filtering exports by the last segment as a prefix.
            // e.g. `load mire::maybe::unwrap` → load `maybe/mod.mire`,
            // filter to `unwrap::*` exports.
            Err(_) if path.len() >= 3 => {
                return self.expand_load_prefix_group(
                    path, alias, line, column,
                    imported_symbol_candidates,
                    expanded, direct_dependencies, dep_set,
                );
            }
            Err(e) => return Err(e),
        };

        let selected = if items.is_some() {
            items
        } else if matches!(self.import_mode, ImportMode::Reachable) {
            load::infer_reachable_import_items(self, &target, None, imported_symbol_candidates)?
        } else {
            None
        };

        let imported = if selected.is_some() {
            self.load_selected_imports(&target, selected.as_deref(), load_span)?
        } else {
            self.load_file(&target, load_span)?
        };

        let prefix = alias.unwrap_or_else(|| {
            if path.len() == 1 && path[0] == "kioto" {
                return String::new();
            }
            path.last().cloned().unwrap_or_default()
        });

        if prefix.is_empty() {
            if dep_set.insert(target.clone()) {
                direct_dependencies.push(target);
            }
            expanded.extend(imported);
        } else {
            let prefixed = prefix_loaded_statements_scoped(imported, &prefix, &target);
            if dep_set.insert(target.clone()) {
                direct_dependencies.push(target);
            }
            expanded.extend(prefixed);
        }
        Ok(())
    }

    /// Fallback for 3+ segment `load` paths that don't resolve as file paths.
    ///
    /// When `load mire::maybe::unwrap` fails to find `unwrap` as a sub-module,
    /// this method loads the parent module (`maybe/mod.mire`) and filters its
    /// exports to those matching the last segment as a prefix (`unwrap::*`).
    ///
    /// This enables function-group loading: `load pkg::module::group` loads
    /// all functions in `module` whose names start with `group::`.
    #[allow(clippy::too_many_arguments)]
    fn expand_load_prefix_group(
        &mut self,
        path: Vec<String>,
        _alias: Option<String>,
        line: usize,
        column: usize,
        _imported_symbol_candidates: &HashSet<String>,
        expanded: &mut Vec<ExpandedStatement>,
        direct_dependencies: &mut Vec<PathBuf>,
        dep_set: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let load_span = Span::new(line, column);
        let group_name = path.last().cloned().unwrap_or_default();
        let parent_path = &path[..path.len() - 1];

        // Resolve the parent module (e.g. `load mire::maybe` → `core/maybe/mod.mire`)
        let target = load::resolve_load_path(self, parent_path, load_span)?;

        // Load the parent to discover its exports
        let all_imported = self.load_file(&target, load_span)?;

        // Collect export names that start with `group_name::`
        let prefix_filter = format!("{}::", group_name);
        let matching_items: Vec<String> = all_imported
            .iter()
            .filter_map(|stmt| {
                crate::incremental::statement_export_name(&stmt.statement)
                    .map(ToString::to_string)
            })
            .filter(|name| name.starts_with(&prefix_filter))
            .collect();

        if matching_items.is_empty() {
            return Err(self.loader_error(load_span, format!(
                "Module '{}' has no exports matching prefix '{}'",
                parent_path.join("::"),
                group_name
            )));
        }

        // Re-load with the filtered items list so transitive deps are resolved
        let imported = self.load_selected_imports(&target, Some(&matching_items), load_span)?;

        // Don't prefix — the flattened names already contain the group prefix
        // (e.g. `unwrap::i64`). Just add the statements directly.
        if dep_set.insert(target.clone()) {
            direct_dependencies.push(target);
        }
        expanded.extend(imported);
        Ok(())
    }

    /// Handle `load!` statement — delegates to `lload` submodule.
    #[allow(clippy::too_many_arguments)]
    fn expand_load_local(
        &mut self,
        canonical: &Path,
        rel_path: Vec<String>,
        absolute: bool,
        line: usize,
        column: usize,
        expanded: &mut Vec<ExpandedStatement>,
        direct_dependencies: &mut Vec<PathBuf>,
        dep_set: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let current_dir = canonical.parent().unwrap_or_else(|| Path::new("."));
        let span = Span::new(line, column);
        let (target, depth) =
            lload::resolve_load_local_target(self, &rel_path, absolute, current_dir, span)?;
        if depth > 2 {
            return Err(self.loader_error(span, format!(
                "load! can only descend 2 levels below owl.toml, but got {} levels",
                depth
            )));
        }
        let namespace = rel_path.last().cloned().unwrap_or_default();
        let imported = self.load_file(&target, span)?;
        let prefixed =
            prefix_loaded_statements_scoped(imported, &namespace, &target);
        if dep_set.insert(target.clone()) {
            direct_dependencies.push(target);
        }
        expanded.extend(prefixed);
        // Keep the `load!` declaration in the program so later passes
        // (e.g. the mandatory `use!` check) can see the imported module.
        expanded.push(ExpandedStatement {
            statement: Statement::LoadLocal { rel_path, absolute, line, column },
            origin: canonical.to_path_buf(),
        });
        Ok(())
    }

    /// Load a file and filter its statements to only the requested imports.
    fn load_selected_imports(
        &mut self,
        path: &Path,
        items: Option<&[String]>,
        span: Span,
    ) -> Result<Vec<ExpandedStatement>> {
        let parsed = load_or_parse_file(self, path)?;
        let has_loads = parsed
            .program
            .statements
            .iter()
            .any(|stmt| matches!(stmt, Statement::Load { .. }));
        if has_loads {
            let loaded = self.load_file(path, span)?;
            return select::select_imported_statements(&loaded, items, path, span);
        }
        self.files.insert(
            path.to_path_buf(),
            LoadedFile {
                hash: parsed.hash,
                direct_dependencies: Vec::new(),
            },
        );
        let expanded: Vec<ExpandedStatement> = parsed
            .program
            .statements
            .into_iter()
            .map(|statement| ExpandedStatement {
                statement,
                origin: path.to_path_buf(),
            })
            .collect();
        select::select_imported_statements(&expanded, items, path, span)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn load_program_from_file(path: &Path) -> Result<Program> {
    Ok(load_program_with_metadata(path)?.program)
}

pub fn load_program_with_metadata(path: &Path) -> Result<LoadedProgram> {
    let settings = CacheSettings::resolve_for(path, Default::default())?;
    load_program_with_metadata_with_settings(path, settings, ImportMode::Reachable)
}

pub fn load_program_with_metadata_with_settings(
    path: &Path,
    settings: CacheSettings,
    import_mode: ImportMode,
) -> Result<LoadedProgram> {
    let canonical = path.canonicalize().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: Span::unknown(),
            message: format!("Could not resolve '{}': {}", path.display(), err),
        })
    })?;

    let project_root = if let Some(root) =
        find_project_root(canonical.parent().unwrap_or_else(|| Path::new(".")))
    {
        root
    } else {
        let fallback = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let manifest_dependencies = HashMap::new();
        let mut cache = IncrementalCache::load_with_settings(&canonical, settings)?;
        let mut resolver = ImportResolver::new(
            fallback.clone(),
            &mut cache,
            import_mode,
            manifest_dependencies,
        );
        let statements = resolver.load_file(&canonical, Span::unknown())?;
        let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
        let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
        let files = std::mem::take(&mut resolver.files);
        let sources = std::mem::take(&mut resolver.sources);
        drop(resolver);
        cache.save()?;
        return Ok(LoadedProgram {
            program: Program {
                file_attributes: vec![],
                annotations: vec![],
                statements: program_statements,
            },
            files,
            statement_origins,
            sources,
        });
    };

    let manifest_dependencies = load_manifest_dependencies(&project_root).unwrap_or_default();
    let mut cache = IncrementalCache::load_with_settings(&canonical, settings)?;
    let mut resolver =
        ImportResolver::new(project_root, &mut cache, import_mode, manifest_dependencies);
    let statements = resolver.load_file(&canonical, Span::unknown())?;
    let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
    let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
    let files = std::mem::take(&mut resolver.files);
    let sources = std::mem::take(&mut resolver.sources);
    drop(resolver);
    cache.save()?;
    Ok(LoadedProgram {
        program: Program {
            file_attributes: vec![],
            annotations: vec![],
            statements: program_statements,
        },
        files,
        statement_origins,
        sources,
    })
}

/// Load program using an already-loaded cache instance.
/// This avoids loading the cache twice when the caller already has one.
pub fn load_program_with_cache(
    path: &Path,
    cache: &mut IncrementalCache,
    import_mode: ImportMode,
) -> Result<LoadedProgram> {
    let canonical = path.canonicalize().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: Span::unknown(),
            message: format!("Could not resolve '{}': {}", path.display(), err),
        })
    })?;

    let project_root = if let Some(root) =
        find_project_root(canonical.parent().unwrap_or_else(|| Path::new(".")))
    {
        root
    } else {
        let fallback = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let manifest_dependencies = HashMap::new();
        let mut resolver =
            ImportResolver::new(fallback.clone(), cache, import_mode, manifest_dependencies);
        let statements = resolver.load_file(&canonical, Span::unknown())?;
        let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
        let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
        return Ok(LoadedProgram {
            program: Program {
                file_attributes: vec![],
                annotations: vec![],
                statements: program_statements,
            },
            files: resolver.files,
            statement_origins,
            sources: resolver.sources,
        });
    };

    let manifest_dependencies = load_manifest_dependencies(&project_root).unwrap_or_default();
    let mut resolver = ImportResolver::new(project_root, cache, import_mode, manifest_dependencies);
    let statements = resolver.load_file(&canonical, Span::unknown())?;
    let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
    let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
    Ok(LoadedProgram {
        program: Program {
            file_attributes: vec![],
            annotations: vec![],
            statements: program_statements,
        },
        files: resolver.files,
        statement_origins,
        sources: resolver.sources,
    })
}
