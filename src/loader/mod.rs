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
mod lload;
mod load;
mod rename;
mod rename_expression;
mod select;

pub(crate) use load::resolve_dependency_root;

use crate::avens::{ImportMode, find_project_root};
use crate::error::{ErrorKind, MireError, Result, Span};
use crate::incremental::{
    CacheSettings, IncrementalCache, LoadedFile, LoadedProgram, statement_export_name,
};
use crate::parser::Program;
use crate::parser::ast::{
    AssignmentTarget, DataType, EnumVariantDef, Expression, Identifier, Literal, Statement,
};
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
    package_registry: HashMap<String, PackageEntry>,
    current_file: Option<String>,
    /// When true, the next file loaded (the entry/root file) has `@[derive]`
    /// attributes expanded on its parsed program before reachable-import
    /// selection. This makes generated impls participate in
    /// `collect_program_dependency_candidates`, so the stdlib functions they
    /// reference (e.g. `str.copy`) are reachable-selected exactly as if the
    /// user had written the impls by hand.
    expand_derives_entry: bool,
    /// Union of the dependency candidates of every module expanded so far in
    /// this load, from the entry file down.
    ///
    /// Reachable-import selection has to see the *whole* chain, not just the
    /// module that wrote the `load`. A consumer calling
    /// `math::float::sign` puts `math.float.sign` in the entry program's
    /// candidates, but the `load kioto::math::float` statement that eventually
    /// pulls the float module in lives in `math/mod.mr` — a file whose own
    /// candidates say nothing about `sign`. Selecting on that file's local set
    /// dropped the export the consumer actually asked for. Accumulating the
    /// ancestors' candidates restores the answer: a module is selected against
    /// everything referenced by any module that can see it.
    ///
    /// The union only ever *widens* selection, so a module may keep a few
    /// exports no consumer asked for. That is safe: every statement is namespaced
    /// by its module, so extra exports cannot collide, and the final expansion's
    /// dedupe pass still collapses identical statements.
    reachable_candidate_scope: HashSet<String>,
    /// Files already visited by the candidate pre-scan, so the scan visits each
    /// canonical path once even when several modules load it.
    candidate_scan_seen: HashSet<PathBuf>,
}

// Local imports may be deeply nested, but a hostile or accidental graph must
// not exhaust compiler memory or stack space. Cycle detection remains exact
// through `active_stack`; these budgets cover acyclic explosions.
const MAX_LOCAL_LOAD_DEPTH: usize = 64;
const MAX_LOADED_FILES: usize = 4096;

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
    ) -> Self {
        Self {
            project_root,
            cache,
            expanded_cache: HashMap::new(),
            active_stack: HashSet::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
            import_mode,
            package_registry: HashMap::new(),
            current_file: None,
            expand_derives_entry: true,
            reachable_candidate_scope: HashSet::new(),
            candidate_scan_seen: HashSet::new(),
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
    /// Collect the dependency candidates of the whole import graph *before* any
    /// reachable selection runs.
    ///
    /// Reachable selection asks "does this module export what somebody asked
    /// for?", and the answer depends on the candidate set. That set used to be
    /// whatever had been collected *so far*, top-down, so an early `load` was
    /// selected against a strictly narrower set than a later one. Two
    /// consequences, both observable:
    ///
    /// * A module pulled in by an early `load` was expanded and cached before a
    ///   later module could contribute its references, so an export only the
    ///   later module asked for was missing. Swapping the two `load` statements
    ///   then changed which exports survived, which is what made the order of
    ///   the statements part of the language's semantics.
    /// * A symbol a dependency needs could be reported as undefined even though
    ///   it was there. `load kioto` before `load testlib` lost kioto's
    ///   `strings::from::i64` (referenced only from inside testlib) and failed
    ///   with `Unknown function 'strings.from.i64'`; the same two loads in the
    ///   opposite order compiled.
    ///
    /// Scanning the graph first makes every selection see every reference in the
    /// program, so the outcome no longer depends on `load` order. The scan is
    /// best-effort: an unresolvable target is skipped here and reported with its
    /// real span by the expansion that follows, so errors keep their location
    /// instead of being reported against the entry file.
    ///
    /// It parses through the same memoized `load_or_parse_file` the expansion
    /// uses, so the extra pass costs no re-parsing of unchanged files and never
    /// populates `expanded_cache`. `@[derive]` expansion is not replayed here:
    /// derived code is intra-module, and widening the scope past what a consumer
    /// asked for is harmless while narrowing it is not.
    fn scan_dependency_candidates(&mut self, path: &Path) {
        let Ok(canonical) = path.canonicalize() else {
            return;
        };
        if self.candidate_scan_seen.len() >= MAX_LOADED_FILES {
            return;
        }
        if !self.candidate_scan_seen.insert(canonical.clone()) {
            return;
        }
        let Ok(parsed) = load_or_parse_file(self, &canonical, None) else {
            // Leave the file marked as seen: the expansion will report the real
            // error, and re-walking a broken graph from every importer would
            // only repeat the failure.
            return;
        };
        self.reachable_candidate_scope
            .extend(collect_program_dependency_candidates(&parsed.program));

        for statement in &parsed.program.statements {
            match statement {
                Statement::Load {
                    path: segments,
                    line,
                    column,
                    ..
                } if !segments.is_empty() && !segments[0].starts_with("__") => {
                    for target in self.scan_load_targets(segments, *line, *column) {
                        self.scan_dependency_candidates(&target);
                    }
                }
                Statement::LoadLocal {
                    rel_path,
                    absolute,
                    line,
                    column,
                } => {
                    let current_dir = canonical.parent().unwrap_or_else(|| Path::new("."));
                    let span = Span::new(*line, *column);
                    if let Ok((target, _depth)) =
                        lload::resolve_load_local_target(self, rel_path, *absolute, current_dir, span)
                    {
                        self.scan_dependency_candidates(&target);
                    }
                }
                _ => {}
            }
        }
    }

    /// Every file a `load` statement can pull in, most specific first.
    ///
    /// A path that does not resolve as written is walked up one segment at a
    /// time, which covers the parent-module fallback in `expand_load`: when
    /// `load pkg::module::group` names no `group` export, the expansion loads
    /// `pkg::module` and filters it, so the scan has to visit that parent too.
    fn scan_load_targets(&mut self, segments: &[String], line: usize, column: usize) -> Vec<PathBuf> {
        let mut targets = Vec::new();
        let mut last_error = None;
        for take in (1..=segments.len()).rev() {
            let prefix = &segments[..take];
            match load::resolve_load_path(self, prefix, Span::new(line, column)) {
                Ok(target) => {
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                    break;
                }
                Err(err) => last_error = Some(err),
            }
        }
        if targets.is_empty() {
            // Nothing resolved. The expansion reports this with a real span, so
            // the scan only keeps the error to avoid an unused-variable warning
            // and stays silent.
            drop(last_error);
        }
        targets
    }

    fn load_file(&mut self, path: &Path, span: Span) -> Result<Vec<ExpandedStatement>> {
        if self.active_stack.len() >= MAX_LOCAL_LOAD_DEPTH {
            return Err(self.loader_error(
                span,
                format!(
                    "local load depth exceeds protected limit {}",
                    MAX_LOCAL_LOAD_DEPTH
                ),
            ));
        }
        if self.files.len() >= MAX_LOADED_FILES {
            return Err(self.loader_error(
                span,
                format!(
                    "import graph exceeds protected limit {} files",
                    MAX_LOADED_FILES
                ),
            ));
        }
        // The outermost call is the program root. Walk the whole import graph
        // once before expanding anything, so reachable selection has every
        // candidate in the program available and the result does not depend on
        // the order of the `load` statements. See
        // `scan_dependency_candidates`.
        if self.active_stack.is_empty() {
            self.scan_dependency_candidates(path);
        }
        let canonical = path.canonicalize().map_err(|err| {
            self.loader_error(
                span,
                format!("Could not resolve '{}': {}", path.display(), err),
            )
        })?;

        if let Some(cached) = self.expanded_cache.get(&canonical) {
            return Ok(cached.clone());
        }

        if !self.active_stack.insert(canonical.clone()) {
            return Err(self.loader_error(
                span,
                format!("Cyclic local load detected at '{}'", canonical.display()),
            ));
        }

        let expanded_source = if self.expand_derives_entry {
            self.expand_derives_entry = false;
            let raw = crate::loader::files::read_source_file(&canonical)?;
            Some(crate::avens::derive::expand_derives_source(&raw))
        } else {
            None
        };
        let parsed = load_or_parse_file(self, &canonical, expanded_source)?;
        // `current_file` is the ambient "which file owns the statement being
        // processed" marker that `loader_error` attaches to diagnostics. A
        // nested expansion must not leak that ownership: without restoring it
        // below, a `load` that fails in this file *after* an earlier `load`
        // expanded many modules is reported against whichever module the
        // previous expansion happened to end in (e.g. a missing dependency
        // written in the root file blamed on the last file of `load kioto`).
        // Only the success path needs restoring: `loader_error` reads the
        // marker when it builds the error, so a propagating `?` has already
        // captured the right filename.
        let previous_file = self
            .current_file
            .replace(canonical.display().to_string());
        let imported_symbol_candidates = collect_program_dependency_candidates(&parsed.program);
        let mut expanded = Vec::new();
        let mut direct_dependencies = Vec::new();
        let mut dep_set = HashSet::new();

        for statement in parsed.program.statements {
            match statement {
                Statement::Load {
                    path,
                    alias,
                    items,
                    line,
                    column,
                } if !path.is_empty() && !path[0].starts_with("__") => {
                    self.expand_load(
                        &canonical,
                        path,
                        alias,
                        items,
                        line,
                        column,
                        &imported_symbol_candidates,
                        &mut expanded,
                        &mut direct_dependencies,
                        &mut dep_set,
                    )?;
                }
                Statement::LoadLocal {
                    rel_path,
                    absolute,
                    line,
                    column,
                } => {
                    self.expand_load_local(
                        &canonical,
                        rel_path,
                        absolute,
                        line,
                        column,
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
        self.current_file = previous_file;
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
                    path,
                    alias,
                    line,
                    column,
                    imported_symbol_candidates,
                    expanded,
                    direct_dependencies,
                    dep_set,
                );
            }
            Err(e) => return Err(e),
        };

        // Selection must consider every ancestor's references, not just this
        // file's. See `reachable_candidate_scope`.
        if self.reachable_candidate_scope.is_empty() {
            self.reachable_candidate_scope = imported_symbol_candidates.clone();
        } else {
            self.reachable_candidate_scope
                .extend(imported_symbol_candidates.iter().cloned());
        }
        let candidate_scope = self.reachable_candidate_scope.clone();

        let selected = if items.is_some() {
            items
        } else if matches!(self.import_mode, ImportMode::Reachable) {
            load::infer_reachable_import_items(self, &target, &path, &candidate_scope)?
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
                crate::incremental::statement_export_name(&stmt.statement).map(ToString::to_string)
            })
            .filter(|name| name.starts_with(&prefix_filter))
            .collect();

        if matching_items.is_empty() {
            return Err(self.loader_error(
                load_span,
                format!(
                    "Module '{}' has no exports matching prefix '{}'",
                    parent_path.join("::"),
                    group_name
                ),
            ));
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
        let (target, _depth) =
            lload::resolve_load_local_target(self, &rel_path, absolute, current_dir, span)?;
        let namespace = rel_path.last().cloned().unwrap_or_default();
        let imported = self.load_file(&target, span)?;
        let prefixed = prefix_loaded_statements_scoped(imported, &namespace, &target);
        if dep_set.insert(target.clone()) {
            direct_dependencies.push(target);
        }
        expanded.extend(prefixed);
        // Keep the `load!` declaration in the program so later passes
        // (e.g. the mandatory `use!` check) can see the imported module.
        expanded.push(ExpandedStatement {
            statement: Statement::LoadLocal {
                rel_path,
                absolute,
                line,
                column,
            },
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
        let parsed = load_or_parse_file(self, path, None)?;
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

/// Multiple `load` statements targeting the same module (e.g. the same
/// dependency loaded from the root file and again from each `load!` group)
/// expand to byte-identical declarations. Keeping only the first copy avoids
/// `X redefined` errors while preserving the union of every consumer's
/// reachable-import selections (subsets differ, so their unique statements
/// survive; only the identical overlap is dropped).
fn dedupe_identical_expanded(statements: Vec<ExpandedStatement>) -> Vec<ExpandedStatement> {
    let mut seen: HashSet<(PathBuf, String)> = HashSet::new();
    let mut result = Vec::with_capacity(statements.len());
    for stmt in statements {
        let key = (stmt.origin.clone(), format!("{:?}", stmt.statement));
        if seen.insert(key) {
            result.push(stmt);
        }
    }
    result
}

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
        let mut cache = IncrementalCache::load_with_settings(&canonical, settings)?;
        let mut resolver = ImportResolver::new(fallback.clone(), &mut cache, import_mode);
        let statements =
            dedupe_identical_expanded(resolver.load_file(&canonical, Span::unknown())?);
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

    let mut cache = IncrementalCache::load_with_settings(&canonical, settings)?;
    let mut resolver = ImportResolver::new(project_root, &mut cache, import_mode);
    let statements = dedupe_identical_expanded(resolver.load_file(&canonical, Span::unknown())?);
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
        let mut resolver = ImportResolver::new(fallback.clone(), cache, import_mode);
        let statements =
            dedupe_identical_expanded(resolver.load_file(&canonical, Span::unknown())?);
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

    let mut resolver = ImportResolver::new(project_root, cache, import_mode);
    let statements = dedupe_identical_expanded(resolver.load_file(&canonical, Span::unknown())?);
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
