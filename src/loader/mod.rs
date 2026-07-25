mod rename;

use crate::avens::{
    ImportMode, MireDependency, find_project_root, load_exports, load_manifest_dependencies,
    load_project_manifest, resolve_export_path,
};
use crate::canonical_fn_name;
use crate::error::{ErrorKind, MireError, Result, Span};
use crate::incremental::{
    CacheSettings, CachedParsedFile, IncrementalCache, LoadedFile, LoadedProgram,
    collect_statement_bindings, collect_statement_dependencies, source_hash, source_hash2,
    statement_export_name,
};
use crate::parser::ast::{
    AssignmentTarget, DataType, EnumVariantDef, Expression, Identifier, Literal, Statement,
};
use crate::parser::{Program, parse};
use rename::prefix_loaded_statements_scoped;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct PackageEntry {
    root: PathBuf,
    entry: String,
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
            span: crate::error::Span::unknown(),
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
            span: crate::error::Span::unknown(),
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

fn owl_home_libs() -> PathBuf {
    if let Some(home) = std::env::var_os("MIRE_OWL_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    PathBuf::from(home).join(".owl").join("libs")
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

        let parsed = self.load_or_parse_file(&canonical)?;
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
                    let load_span = Span::new(line, column);
                    let target = self.resolve_load_path(&path, load_span)?;

                    let selected = if items.is_some() {
                        items
                    } else if matches!(self.import_mode, ImportMode::Reachable) {
                        self.infer_reachable_import_items(
                            &target,
                            None,
                            &imported_symbol_candidates,
                        )?
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
                }
                Statement::LoadLocal { rel_path, absolute, line, column } => {
                    let current_dir = canonical.parent().unwrap_or_else(|| Path::new("."));
                    let span = Span::new(line, column);
                    let (target, depth) =
                        self.resolve_load_local_target(&rel_path, absolute, current_dir, span)?;
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
                        origin: canonical.clone(),
                    });
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

    fn resolve_package(&mut self, name: &str, span: Span) -> Result<(PathBuf, String)> {
        if let Some(entry) = self.package_registry.get(name) {
            return Ok((entry.root.clone(), entry.entry.clone()));
        }
        let package_root = if let Some(dep) = self.manifest_dependencies.get(name) {
            match dep {
                MireDependency::PathOnly { path } | MireDependency::WithPath { path, .. } => {
                    let p = PathBuf::from(path);
                    if p.is_absolute() {
                        p
                    } else {
                        self.project_root.join(p)
                    }
                }
                MireDependency::Simple { .. } => owl_home_libs().join(name),
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
                    self.project_root.join("../kioto")
                }
            }
        } else {
            return Err(self.loader_error(span, format!(
                "Package '{}' not found in [dependencies] of {}",
                name,
                self.project_root.join("owl.toml").display()
            )));
        };

        let canonical_root = package_root.canonicalize().map_err(|err| {
            self.loader_error(span, format!(
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
                self.manifest_dependencies
                    .entry(dep_name.clone())
                    .or_insert_with(|| dep.clone());
            }
        }

        self.package_registry.insert(
            name.to_string(),
            PackageEntry {
                root: canonical_root.clone(),
                entry: entry.clone(),
            },
        );

        Ok((canonical_root, entry))
    }

    /// Resolve a `load!` (LoadLocal) target relative to the project root (absolute)
    /// or the current file's directory (relative). Tries `<path>/main.mire`,
    /// `<path>/mod.mire`, then `<path>.mire`. Returns the resolved file and the
    /// directory depth below the project root (used for the 2-level limit).
    fn resolve_load_local_target(
        &self,
        rel_path: &[String],
        absolute: bool,
        current_dir: &Path,
        span: Span,
    ) -> Result<(PathBuf, usize)> {
        let base = if absolute {
            self.project_root.clone()
        } else {
            current_dir.to_path_buf()
        };
        let joined: PathBuf = rel_path.iter().collect();
        let candidate_dir = base.join(&joined);
        let candidates = [
            candidate_dir.join("main.mire"),
            candidate_dir.join("mod.mire"),
            candidate_dir.with_extension("mire"),
        ];
        let target = candidates
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| {
                self.loader_error(span, format!("load! target '{}' not found", rel_path.join("/")))
            })?;
        let depth = target
            .parent()
            .and_then(|p| p.strip_prefix(&self.project_root).ok())
            .map(|rel| {
                rel.components()
                    .filter(|c| {
                        !matches!(c, std::path::Component::CurDir | std::path::Component::ParentDir)
                    })
                    .count()
            })
            .unwrap_or(0);
        Ok((target, depth))
    }

    fn resolve_load_path(&mut self, segments: &[String], span: Span) -> Result<PathBuf> {
        let (mut current_root, entry) = self.resolve_package(&segments[0], span)?;
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
                    self.loader_error(span, format!("Package '{}' has no export '{}'", segments[0], segment))
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
                return Err(self.loader_error(span, format!(
                    "Cannot resolve '{}': '{}' has no sub-exports",
                    segments[i + 1..].join("::"),
                    segment
                )));
            }
        }

        unreachable!()
    }

    fn infer_reachable_import_items(
        &mut self,
        path: &Path,
        module_prefix: Option<&str>,
        candidates: &HashSet<String>,
    ) -> Result<Option<Vec<String>>> {
        let parsed = self.load_or_parse_file(path)?;
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

    fn load_or_parse_file(&mut self, path: &Path) -> Result<ResolvedFile> {
        let source = read_source_file(path)?;
        self.sources.insert(path.to_path_buf(), source.clone());
        let hash = source_hash(&source);
        let hash2 = source_hash2(&source);
        if let Some(cached) = self.cache.cached_file(path, hash, hash2) {
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
        self.cache.store_file(
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

    fn load_selected_imports(
        &mut self,
        path: &Path,
        items: Option<&[String]>,
        span: Span,
    ) -> Result<Vec<ExpandedStatement>> {
        let parsed = self.load_or_parse_file(path)?;
        let has_loads = parsed
            .program
            .statements
            .iter()
            .any(|stmt| matches!(stmt, Statement::Load { .. }));
        if has_loads {
            let loaded = self.load_file(path, span)?;
            return select_imported_statements(&loaded, items, path, span);
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
        select_imported_statements(&expanded, items, path, span)
    }
}

fn collect_program_dependency_candidates(program: &Program) -> HashSet<String> {
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

fn select_imported_statements(
    statements: &[ExpandedStatement],
    items: Option<&[String]>,
    import_path: &Path,
    span: Span,
) -> Result<Vec<ExpandedStatement>> {
    if let Some(items) = items {
        let mut selected_indices = Vec::new();
        let mut selected = HashSet::new();
        for item in items {
            let statement_idx = statements
                .iter()
                .enumerate()
                .find(|statement| {
                    statement_export_name(&statement.1.statement) == Some(item.as_str())
                })
                .map(|(idx, _)| idx)
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        span,
                        message: format!(
                            "Local load '{}' does not export '{}'",
                            import_path.display(),
                            item
                        ),
                    })
                })?;
            if selected.insert(statement_idx) {
                selected_indices.push(statement_idx);
            }
        }

        let mut cursor = 0usize;
        while cursor < selected_indices.len() {
            let idx = selected_indices[cursor];
            cursor += 1;

            let mut deps = Vec::new();
            collect_statement_dependencies(&statements[idx].statement, &mut deps);
            for dependency in deps {
                for candidate in [
                    Some(dependency.as_str()),
                    dependency.rsplit_once('.').map(|(_, tail)| tail),
                ] {
                    let Some(candidate_name) = candidate else {
                        continue;
                    };
                    for (dep_idx, statement) in statements.iter().enumerate() {
                        let export_name = statement_export_name(&statement.statement);
                        let internal_name = match &statement.statement {
                            Statement::ExternFunction { name, .. }
                            | Statement::ExternLib { name, .. } => Some(name.as_str()),
                            Statement::Let { name, .. } => Some(name.as_str()),
                            Statement::Assignment {
                                target: AssignmentTarget::Variable(name),
                                ..
                            } => Some(name.as_str()),
                            _ => None,
                        };
                        if (export_name == Some(candidate_name)
                            || internal_name == Some(candidate_name))
                            && selected.insert(dep_idx)
                        {
                            selected_indices.push(dep_idx);
                        }
                    }
                }
            }
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
            let mut deps = Vec::new();
            collect_statement_dependencies(&statements[idx].statement, &mut deps);
            for dependency in deps {
                for candidate in [
                    Some(dependency.as_str()),
                    dependency.rsplit_once('.').map(|(_, tail)| tail),
                ] {
                    let Some(candidate_name) = candidate else {
                        continue;
                    };
                    for (dep_idx, statement) in statements.iter().enumerate() {
                        let export_name = statement_export_name(&statement.statement);
                        let internal_name = match &statement.statement {
                            Statement::ExternFunction { name, .. }
                            | Statement::ExternLib { name, .. } => Some(name.as_str()),
                            Statement::Let { name, .. } => Some(name.as_str()),
                            Statement::Assignment {
                                target: AssignmentTarget::Variable(name),
                                ..
                            } => Some(name.as_str()),
                            _ => None,
                        };
                        if (export_name == Some(candidate_name)
                            || internal_name == Some(candidate_name))
                            && selected.insert(dep_idx)
                        {
                            selected_indices.push(dep_idx);
                        }
                    }
                }
            }
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

fn read_source_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not read '{}': {}", path.display(), err),
        })
    })
}

struct ResolvedFile {
    hash: u64,
    program: Program,
    exports: Vec<String>,
}

#[derive(Clone)]
struct ExpandedStatement {
    statement: Statement,
    origin: PathBuf,
}
