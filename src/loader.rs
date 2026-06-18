use crate::avens::{
    find_project_root, load_exports, load_manifest_dependencies, load_project_manifest,
    resolve_export_path, ImportMode, MireDependency,
};
use crate::error::{ErrorKind, MireError, Result};
use crate::incremental::{
    collect_statement_bindings, collect_statement_dependencies, source_hash, source_hash2,
    statement_export_name, CacheSettings, CachedParsedFile, IncrementalCache, LoadedFile,
    LoadedProgram,
};
use crate::parser::ast::{
    AssignmentTarget, DataType, EnumVariantDef, Expression, Identifier, Literal, Statement,
};
use crate::parser::{parse, Program};
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
            message: format!("Could not resolve '{}': {}", path.display(), err),
        })
    })?;

    let project_root = if let Some(root) =
        find_project_root(canonical.parent().unwrap_or_else(|| Path::new(".")))
    {
        root
    } else {
        let fallback = canonical.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let manifest_dependencies = HashMap::new();
        let mut resolver = ImportResolver::new(
            fallback.clone(),
            IncrementalCache::load_with_settings(&canonical, settings)?,
            import_mode,
            manifest_dependencies,
        );
        let statements = resolver.load_file(&canonical)?;
        resolver.cache.save()?;
        let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
        let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
        return Ok(LoadedProgram {
            program: Program {
                statements: program_statements,
            },
            files: resolver.files,
            statement_origins,
            sources: resolver.sources,
        });
    };

    let manifest_dependencies = load_manifest_dependencies(&project_root).unwrap_or_default();
    let mut resolver = ImportResolver::new(
        project_root,
        IncrementalCache::load_with_settings(&canonical, settings)?,
        import_mode,
        manifest_dependencies,
    );
    let statements = resolver.load_file(&canonical)?;
    resolver.cache.save()?;
    let statement_origins = statements.iter().map(|stmt| stmt.origin.clone()).collect();
    let program_statements = statements.into_iter().map(|stmt| stmt.statement).collect();
    Ok(LoadedProgram {
        program: Program {
            statements: program_statements,
        },
        files: resolver.files,
        statement_origins,
        sources: resolver.sources,
    })
}

fn owl_home_modules() -> PathBuf {
    if let Some(home) = std::env::var_os("MIRE_OWL_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    PathBuf::from(home).join(".owl").join("modules")
}

struct ImportResolver {
    project_root: PathBuf,
    cache: IncrementalCache,
    expanded_cache: HashMap<PathBuf, Vec<ExpandedStatement>>,
    active_stack: HashSet<PathBuf>,
    files: HashMap<PathBuf, LoadedFile>,
    sources: HashMap<PathBuf, String>,
    import_mode: ImportMode,
    manifest_dependencies: HashMap<String, MireDependency>,
    package_registry: HashMap<String, PackageEntry>,
}

impl ImportResolver {
    fn new(
        project_root: PathBuf,
        cache: IncrementalCache,
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
        }
    }

    fn load_file(&mut self, path: &Path) -> Result<Vec<ExpandedStatement>> {
        let canonical = path.canonicalize().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not resolve '{}': {}", path.display(), err),
            })
        })?;

        if let Some(cached) = self.expanded_cache.get(&canonical) {
            return Ok(cached.clone());
        }

        if !self.active_stack.insert(canonical.clone()) {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!("Cyclic local load detected at '{}'", canonical.display()),
            }));
        }

        let parsed = self.load_or_parse_file(&canonical)?;
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
                } if !path.is_empty() && !path[0].starts_with("__") => {
                    let target = self.resolve_load_path(&path)?;

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
                        self.load_selected_imports(&target, selected.as_deref())?
                    } else {
                        self.load_file(&target)?
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
                        let prefixed =
                            prefix_loaded_statements_scoped(imported, &prefix, &target);
                        if dep_set.insert(target.clone()) {
                            direct_dependencies.push(target);
                        }
                        expanded.extend(prefixed);
                    }
                }
                Statement::Use { path } => {
                    // `use <pkg>::...` resolves via TOML chain.
                    // Module-level: load whole file, prefix with last segment.
                    // Item-level: last segment is a symbol, inject without prefix.
                    let (target, items) = self.resolve_use_path(&path)?;
                    if items.is_empty() {
                        let prefix = path.last().cloned().unwrap_or_default();
                        let imported = self.load_file(&target)?;
                        let prefixed =
                            prefix_loaded_statements_scoped(imported, &prefix, &target);
                        if dep_set.insert(target.clone()) {
                            direct_dependencies.push(target);
                        }
                        expanded.extend(prefixed);
                    } else {
                        let imported = self.load_selected_imports(&target, Some(&items))?;
                        if dep_set.insert(target.clone()) {
                            direct_dependencies.push(target);
                        }
                        expanded.extend(imported);
                    }
                }
                Statement::UseModule { .. } => {
                    // `use <name>` (single ident). Currently no-op.
                    // TODO: resolve `name` against known packages.
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

    fn resolve_package(&mut self, name: &str) -> Result<(PathBuf, String)> {
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
                MireDependency::Simple { .. } => owl_home_modules().join(name),
            }
        } else if name == "kioto" {
            let home_path = owl_home_modules().join("kioto");
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
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Package '{}' not found in [dependencies] of {}",
                    name,
                    self.project_root.join("owl.toml").display()
                ),
            }));
        };

        let canonical_root = package_root.canonicalize().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Could not resolve package '{}' at '{}': {}",
                    name,
                    package_root.display(),
                    err
                ),
            })
        })?;

        let manifest = load_project_manifest(&canonical_root)?;
        let entry = manifest
            .as_ref()
            .map(|m| m.project.entry.clone())
            .unwrap_or_else(|| "mod.mire".to_string());

        if let Some(ref m) = manifest {
            for (dep_name, dep) in &m.dependencies.entries {
                self.manifest_dependencies.entry(dep_name.clone()).or_insert_with(|| dep.clone());
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

    fn resolve_load_path(&mut self, segments: &[String]) -> Result<PathBuf> {
        let (mut current_root, entry) = self.resolve_package(&segments[0])?;
        let mut current_exports = load_exports(&current_root).unwrap_or_default();

        if segments.len() == 1 {
            let direct = current_root.join(&entry);
            if direct.exists() {
                return Ok(direct);
            }
            if let Some(export_path) =
                resolve_export_path(&current_exports, &current_root, &segments[0])
            {
                if export_path.exists() {
                    return Ok(export_path);
                }
            }
            return Ok(direct);
        }

        for i in 1..segments.len() {
            let segment = &segments[i];
            let is_last = i == segments.len() - 1;

            let target = resolve_export_path(&current_exports, &current_root, segment)
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Package '{}' has no export '{}'",
                            segments[0], segment
                        ),
                    })
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
                return Err(MireError::new(ErrorKind::Runtime {
                    message: format!(
                        "Cannot resolve '{}': '{}' has no sub-exports",
                        segments[i + 1..].join("::"),
                        segment
                    ),
                }));
            }
        }

        unreachable!()
    }

    fn resolve_use_path(&mut self, segments: &[String]) -> Result<(PathBuf, Vec<String>)> {
        // Returns (file_path, items_to_extract).
        // items is empty → module-level import (prefix with last segment).
        // items non-empty → item-level import (inject bare, no prefix).
        if let Ok(file) = self.resolve_load_path(segments) {
            return Ok((file, vec![]));
        }
        for split in (1..segments.len()).rev() {
            if let Ok(file) = self.resolve_load_path(&segments[..split]) {
                return Ok((file, segments[split..].to_vec()));
            }
        }
        Err(MireError::new(ErrorKind::Runtime {
            message: format!(
                "Cannot resolve use path '{}': package or export not found",
                segments.join("::")
            ),
        }))
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
            let export_tail = export
                .rsplit_once('.')
                .map_or(export.as_str(), |(_, tail)| tail);
            let prefixed = module_prefix.map(|prefix| format!("{prefix}.{export_tail}"));
            let prefixed_double_colon =
                module_prefix.map(|prefix| format!("{prefix}::{export_tail}"));
            if candidates.contains(export)
                || candidates.contains(export_tail)
                || prefixed
                    .as_ref()
                    .is_some_and(|value| candidates.contains(value))
                || prefixed_double_colon
                    .as_ref()
                    .is_some_and(|value| candidates.contains(value))
            {
                selected.push(export_tail.to_string());
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
            return Ok(ResolvedFile::from_cached(cached, source));
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
    ) -> Result<Vec<ExpandedStatement>> {
        let parsed = self.load_or_parse_file(path)?;
        let has_loads = parsed.program.statements.iter().any(|stmt| {
            matches!(stmt, Statement::Load { .. })
        });
        if has_loads {
            let loaded = self.load_file(path)?;
            return select_imported_statements(&loaded, items, path);
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
        select_imported_statements(&expanded, items, path)
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

fn prefix_loaded_statements_scoped(
    statements: Vec<ExpandedStatement>,
    module_name: &str,
    module_path: &Path,
) -> Vec<ExpandedStatement> {
    let mut symbols_by_prefix: HashMap<String, HashSet<String>> = HashMap::new();
    for statement in &statements {
        let prefix = statement_prefix(module_name, module_path, &statement.origin);
        if let Some(name) = statement_export_name(&statement.statement) {
            symbols_by_prefix
                .entry(prefix)
                .or_default()
                .insert(name.to_string());
        }
    }

    statements
        .into_iter()
        .map(|mut statement| {
            let prefix = statement_prefix(module_name, module_path, &statement.origin);
            if prefix.is_empty() {
                return statement;
            }
            let module_symbols = symbols_by_prefix.get(&prefix).cloned().unwrap_or_default();
            let renamer = ModuleRenamer {
                prefix: &prefix,
                module_symbols: &module_symbols,
            };
            statement.statement = renamer.rename_statement(statement.statement, true);
            statement
        })
        .collect()
}

fn statement_prefix(module_name: &str, module_path: &Path, origin: &Path) -> String {
    if origin == module_path {
        let file_stem = module_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if file_stem.starts_with('_') {
            return String::new();
        }
        return module_name.to_string();
    }

    let base = module_path.parent().unwrap_or(module_path);
    let relative = origin.strip_prefix(base).ok().unwrap_or(origin);
    let mut parts = Vec::new();
    for component in relative.components() {
        let part = component.as_os_str().to_string_lossy().to_string();
        if !part.is_empty() {
            parts.push(part);
        }
    }

    if parts.is_empty() {
        return module_name.to_string();
    }

    let file_name = parts.pop().unwrap();
    let file_stem = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&file_name)
        .to_string();

    if file_stem.starts_with('_') {
        return String::new();
    }

    if file_stem == "mod" {
        if !parts.is_empty() && (parts[0] == "core" || parts[0] == "ext") {
            parts.remove(0);
        }
        if parts.is_empty() {
            module_name.to_string()
        } else {
            parts.join(".")
        }
    } else {
        if !parts.is_empty() && (parts[0] == "core" || parts[0] == "ext") {
            parts.remove(0);
        }
        parts.push(file_stem);
        if parts.is_empty() {
            module_name.to_string()
        } else {
            parts.join(".")
        }
    }
}

struct ModuleRenamer<'a> {
    prefix: &'a str,
    module_symbols: &'a HashSet<String>,
}

impl<'a> ModuleRenamer<'a> {
    fn rename_statement(&self, statement: Statement, top_level: bool) -> Statement {
        let mut scope_stack = vec![HashSet::new()];
        self.rename_statement_with_scope(statement, &mut scope_stack, top_level)
    }

    fn rename_statement_with_scope(
        &self,
        statement: Statement,
        scope_stack: &mut Vec<HashSet<String>>,
        top_level: bool,
    ) -> Statement {
        match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_constant,
                is_mutable,
                is_static,
                visibility,
                name_line,
                name_column,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let data_type = self.rename_data_type(data_type, scope_stack);
                let value = value.map(|expr| self.rename_expression(expr, scope_stack));
                Statement::Let {
                    name,
                    data_type,
                    value,
                    is_constant,
                    is_mutable,
                    is_static,
                    visibility,
                    name_line,
                    name_column,
                }
            }
            Statement::Assignment {
                target,
                value,
                is_mutable,
            } => Statement::Assignment {
                target: self.rename_assignment_target(target, scope_stack),
                value: self.rename_expression(value, scope_stack),
                is_mutable,
            },
            Statement::Function {
                name,
                type_params,
                type_param_bounds,
                params,
                body,
                return_type,
                visibility,
                is_method,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                    scope.extend(params.iter().map(|(name, _)| name.clone()));
                }
                let params = params
                    .into_iter()
                    .map(|(param_name, param_type)| {
                        (param_name, self.rename_data_type(param_type, scope_stack))
                    })
                    .collect();
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let return_type = self.rename_data_type(return_type, scope_stack);
                let body = self.rename_statement_block(body, &mut body_scope);
                Statement::Function {
                    name,
                    type_params,
                    type_param_bounds,
                    params,
                    body,
                    return_type,
                    visibility,
                    is_method,
                }
            }
            Statement::Return(expr) => {
                Statement::Return(expr.map(|expr| self.rename_expression(expr, scope_stack)))
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => Statement::If {
                condition: self.rename_expression(condition, scope_stack),
                then_branch: self.rename_statement_block(then_branch, &mut scope_stack.clone()),
                else_branch: else_branch
                    .map(|branch| self.rename_statement_block(branch, &mut scope_stack.clone())),
            },
            Statement::While { condition, body } => Statement::While {
                condition: self.rename_expression(condition, scope_stack),
                body: self.rename_statement_block(body, &mut scope_stack.clone()),
            },
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.insert(variable.clone());
                    if let Some(index) = &index {
                        scope.insert(index.clone());
                    }
                }
                Statement::For {
                    variable,
                    index,
                    iterable: self.rename_expression(iterable, scope_stack),
                    body: self.rename_statement_block(body, &mut body_scope),
                }
            }
            Statement::Expression(expr) => {
                Statement::Expression(self.rename_expression(expr, scope_stack))
            }
            Statement::Break => Statement::Break,
            Statement::Continue => Statement::Continue,
            Statement::Find {
                variable,
                iterable,
                body,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.insert(variable.clone());
                }
                Statement::Find {
                    variable,
                    iterable: self.rename_expression(iterable, scope_stack),
                    body: self.rename_statement_block(body, &mut body_scope),
                }
            }
            Statement::Match {
                value,
                cases,
                default,
            } => {
                let value = self.rename_expression(value, scope_stack);
                let cases = cases
                    .into_iter()
                    .map(|(pattern, body)| {
                        let pattern = self.rename_match_pattern(pattern, scope_stack);
                        let mut case_scope = scope_stack.clone();
                        if let Some(scope) = case_scope.last_mut() {
                            scope.extend(match_pattern_bindings(&pattern));
                        }
                        (pattern, self.rename_statement_block(body, &mut case_scope))
                    })
                    .collect();
                let default = self.rename_statement_block(default, &mut scope_stack.clone());
                Statement::Match {
                    value,
                    cases,
                    default,
                }
            }
            Statement::Type {
                name,
                type_params,
                type_param_bounds,
                parent,
                fields,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let mut fields_scope = scope_stack.clone();
                if let Some(scope) = fields_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                }
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let parent = parent.map(|parent| self.rename_type_name(parent, scope_stack));
                let fields = self.rename_statement_block(fields, &mut fields_scope);
                Statement::Type {
                    name,
                    type_params,
                    type_param_bounds,
                    parent,
                    fields,
                }
            }
            Statement::Skill { name, methods } => Statement::Skill {
                name: self.rename_decl_name(name, scope_stack, top_level),
                methods: methods
                    .into_iter()
                    .map(|mut method| {
                        method.params = method
                            .params
                            .into_iter()
                            .map(|(param_name, param_type)| {
                                (param_name, self.rename_data_type(param_type, scope_stack))
                            })
                            .collect();
                        method.return_type = self.rename_data_type(method.return_type, scope_stack);
                        method
                    })
                    .collect(),
            },
            Statement::Impl {
                trait_name,
                type_name,
                type_params,
                type_param_bounds,
                methods,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                }
                let trait_name = trait_name.map(|name| self.rename_type_name(name, scope_stack));
                let type_name = self.rename_type_name(type_name, scope_stack);
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let methods = self.rename_statement_block(methods, &mut body_scope);
                Statement::Impl {
                    trait_name,
                    type_name,
                    type_params,
                    type_param_bounds,
                    methods,
                }
            }
            Statement::ExternLib { name, path } => Statement::ExternLib {
                name: self.rename_decl_name(name, scope_stack, top_level),
                path,
            },
            Statement::ExternFunction {
                name,
                lib_name,
                params,
                return_type,
            } => Statement::ExternFunction {
                name: self.rename_extern_name(name, scope_stack, top_level, &lib_name),
                lib_name,
                params: params
                    .into_iter()
                    .map(|(param_name, param_type)| {
                        (param_name, self.rename_data_type(param_type, scope_stack))
                    })
                    .collect(),
                return_type: self.rename_data_type(return_type, scope_stack),
            },
            Statement::Unsafe { body } => Statement::Unsafe {
                body: self.rename_statement_block(body, &mut scope_stack.clone()),
            },
            Statement::Asm { instructions } => Statement::Asm {
                instructions: instructions
                    .into_iter()
                    .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                    .collect(),
            },
            Statement::Load {
                path,
                alias,
                items,
            } => Statement::Load {
                path,
                alias,
                items,
            },
            Statement::Module { name } => Statement::Module {
                name: self.rename_decl_name(name, scope_stack, top_level),
            },
            Statement::Drop { value } => Statement::Drop {
                value: self.rename_expression(value, scope_stack),
            },
            Statement::New {
                value,
                declared_type,
            } => Statement::New {
                value: value.map(|expr| self.rename_expression(expr, scope_stack)),
                declared_type: self.rename_data_type(declared_type, scope_stack),
            },
            Statement::Own { value, inner_type } => Statement::Own {
                value: value.map(|expr| self.rename_expression(expr, scope_stack)),
                inner_type: self.rename_data_type(inner_type, scope_stack),
            },
            Statement::Move { target, value } => Statement::Move {
                target: self.rename_decl_name(target, scope_stack, top_level),
                value: self.rename_expression(value, scope_stack),
            },
            Statement::Enum {
                name,
                type_params,
                type_param_bounds,
                variants,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let variants = variants
                    .into_iter()
                    .map(|variant| self.rename_enum_variant(variant, &name, scope_stack))
                    .collect();
                Statement::Enum {
                    name,
                    type_params,
                    type_param_bounds,
                    variants,
                }
            }
            Statement::Query {
                table,
                bindings,
                ops,
                joins,
                group_by,
            } => Statement::Query {
                table,
                bindings,
                ops: ops
                    .into_iter()
                    .map(|op| self.rename_query_op(op, scope_stack))
                    .collect(),
                joins,
                group_by,
            },
            Statement::Use { path } => Statement::Use { path },
            Statement::UseModule { name } => Statement::UseModule {
                name: self.rename_decl_name(name, scope_stack, top_level),
            },
        }
    }

    fn rename_statement_block(
        &self,
        statements: Vec<Statement>,
        scope_stack: &mut Vec<HashSet<String>>,
    ) -> Vec<Statement> {
        let mut renamed = Vec::with_capacity(statements.len());
        for statement in statements {
            let renamed_statement = self.rename_statement_with_scope(statement, scope_stack, false);
            let bindings = statement_bindings(&renamed_statement);
            if let Some(scope) = scope_stack.last_mut() {
                scope.extend(bindings);
            }
            renamed.push(renamed_statement);
        }
        renamed
    }

    fn rename_decl_name(
        &self,
        name: String,
        scope_stack: &[HashSet<String>],
        top_level: bool,
    ) -> String {
        if top_level && self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    fn rename_extern_name(
        &self,
        name: String,
        scope_stack: &[HashSet<String>],
        top_level: bool,
        lib_name: &str,
    ) -> String {
        if lib_name == "c" {
            name
        } else if top_level && self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    fn rename_type_name(&self, name: String, scope_stack: &[HashSet<String>]) -> String {
        if self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    fn should_prefix(&self, name: &str, scope_stack: &[HashSet<String>]) -> bool {
        // Skip names that already contain a prefix (introduced by a prior pass).
        // Function names in mire are plain identifiers without dots natively,
        // so a dot means the name was already prefixed by a nested load.
        self.module_symbols.contains(name)
            && !is_shadowed(scope_stack, name)
            && !name.contains('.')
    }

    fn rename_data_type(&self, data_type: DataType, scope_stack: &[HashSet<String>]) -> DataType {
        match data_type {
            DataType::StructNamed(name) => {
                DataType::StructNamed(self.rename_type_name(name, scope_stack))
            }
            DataType::EnumNamed(name) => {
                DataType::EnumNamed(self.rename_type_name(name, scope_stack))
            }
            DataType::DynTrait { trait_name } => DataType::DynTrait {
                trait_name: self.rename_type_name(trait_name, scope_stack),
            },
            DataType::Vector {
                element_type,
                dynamic,
            } => DataType::Vector {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
                dynamic,
            },
            DataType::Slice { element_type } => DataType::Slice {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
            },
            DataType::Result { ok, err } => DataType::Result {
                ok: Box::new(self.rename_data_type(*ok, scope_stack)),
                err: Box::new(self.rename_data_type(*err, scope_stack)),
            },
            DataType::Map {
                key_type,
                value_type,
            } => DataType::Map {
                key_type: Box::new(self.rename_data_type(*key_type, scope_stack)),
                value_type: Box::new(self.rename_data_type(*value_type, scope_stack)),
            },
            DataType::Array { element_type, size } => DataType::Array {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
                size,
            },
            DataType::Ref { inner } => DataType::Ref {
                inner: Box::new(self.rename_data_type(*inner, scope_stack)),
            },
            DataType::RefMut { inner } => DataType::RefMut {
                inner: Box::new(self.rename_data_type(*inner, scope_stack)),
            },
            other => other,
        }
    }

    fn rename_assignment_target(
        &self,
        target: AssignmentTarget,
        scope_stack: &[HashSet<String>],
    ) -> AssignmentTarget {
        match target {
            AssignmentTarget::Variable(name) => {
                AssignmentTarget::Variable(self.rename_type_name(name, scope_stack))
            }
            AssignmentTarget::Field(path) => {
                let mut parts = path.split('.').map(ToString::to_string).collect::<Vec<_>>();
                if let Some(root) = parts.first_mut() {
                    *root = self.rename_type_name(root.clone(), scope_stack);
                }
                AssignmentTarget::Field(parts.join("."))
            }
            AssignmentTarget::Index { target, index } => AssignmentTarget::Index {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                index: Box::new(self.rename_expression(*index, scope_stack)),
            },
        }
    }

    fn rename_match_pattern(
        &self,
        pattern: Expression,
        scope_stack: &[HashSet<String>],
    ) -> Expression {
        match pattern {
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
            } => Expression::EnumVariant {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                payloads: payloads
                    .into_iter()
                    .map(|payload| match payload {
                        Expression::Identifier(_) => payload,
                        other => self.rename_expression(other, scope_stack),
                    })
                    .collect(),
                data_type,
            },
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
            } => Expression::EnumVariantPath {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                data_type,
            },
            Expression::Call {
                name,
                args,
                type_args,
                data_type,
            } if name == "__match_guard" || name == "__match_or" => Expression::Call {
                name,
                args: args
                    .into_iter()
                    .map(|arg| self.rename_match_pattern(arg, scope_stack))
                    .collect(),
                type_args: type_args
                    .into_iter()
                    .map(|data_type| self.rename_data_type(data_type, scope_stack))
                    .collect(),
                data_type,
            },
            other => self.rename_expression(other, scope_stack),
        }
    }

    fn rename_expression(
        &self,
        expression: Expression,
        scope_stack: &[HashSet<String>],
    ) -> Expression {
        match expression {
            Expression::Identifier(Identifier {
                name,
                data_type,
                line,
                column,
            }) => Expression::Identifier(Identifier {
                name: self.rename_type_name(name, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
                line,
                column,
            }),
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } => Expression::BinaryOp {
                operator,
                left: Box::new(self.rename_expression(*left, scope_stack)),
                right: Box::new(self.rename_expression(*right, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::UnaryOp {
                operator,
                operand,
                data_type,
            } => Expression::UnaryOp {
                operator,
                operand: Box::new(self.rename_expression(*operand, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::NamedArg {
                name,
                value,
                data_type,
            } => Expression::NamedArg {
                name,
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Call {
                name,
                args,
                type_args,
                data_type,
            } => {
                let name = self.rename_type_name(name, scope_stack);
                Expression::Call {
                    name,
                    args: args
                        .into_iter()
                        .map(|arg| self.rename_expression(arg, scope_stack))
                        .collect(),
                    type_args: type_args
                        .into_iter()
                        .map(|data_type| self.rename_data_type(data_type, scope_stack))
                        .collect(),
                    data_type: self.rename_data_type(data_type, scope_stack),
                }
            }
            Expression::List {
                elements,
                element_type,
                data_type,
            } => Expression::List {
                elements: elements
                    .into_iter()
                    .map(|element| self.rename_expression(element, scope_stack))
                    .collect(),
                element_type: self.rename_data_type(element_type, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Dict {
                entries,
                key_type,
                value_type,
                data_type,
            } => Expression::Dict {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            self.rename_expression(key, scope_stack),
                            self.rename_expression(value, scope_stack),
                        )
                    })
                    .collect(),
                key_type: self.rename_data_type(key_type, scope_stack),
                value_type: self.rename_data_type(value_type, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Tuple {
                elements,
                data_type,
            } => Expression::Tuple {
                elements: elements
                    .into_iter()
                    .map(|element| self.rename_expression(element, scope_stack))
                    .collect(),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Index {
                target,
                index,
                data_type,
            } => Expression::Index {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                index: Box::new(self.rename_expression(*index, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::MemberAccess {
                target,
                member,
                data_type,
            } => Expression::MemberAccess {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                member,
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => {
                let mut body_scope = scope_stack.to_vec();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(params.iter().map(|(name, _)| name.clone()));
                }
                Expression::Closure {
                    params: params
                        .into_iter()
                        .map(|(name, data_type)| {
                            (name, self.rename_data_type(data_type, scope_stack))
                        })
                        .collect(),
                    body: self.rename_statement_block(body, &mut body_scope),
                    return_type: self.rename_data_type(return_type, scope_stack),
                    capture,
                }
            }
            Expression::Reference {
                expr,
                is_mutable,
                data_type,
                referenced_type,
            } => Expression::Reference {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                is_mutable,
                data_type: self.rename_data_type(data_type, scope_stack),
                referenced_type: self.rename_data_type(referenced_type, scope_stack),
            },
            Expression::Dereference { expr, data_type } => Expression::Dereference {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Box { value, data_type } => Expression::Box {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Pipeline {
                input,
                stage,
                safe,
                data_type,
            } => Expression::Pipeline {
                input: Box::new(self.rename_expression(*input, scope_stack)),
                stage: Box::new(self.rename_expression(*stage, scope_stack)),
                safe,
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Try { expr, data_type } => Expression::Try {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Ok { value, data_type } => Expression::Ok {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Err { value, data_type } => Expression::Err {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Match {
                value,
                cases,
                default,
                data_type,
            } => {
                let value = self.rename_expression(*value, scope_stack);
                let cases = cases
                    .into_iter()
                    .map(|(pattern, body)| {
                        let pattern = self.rename_match_pattern(pattern, scope_stack);
                        let mut case_scope = scope_stack.to_vec();
                        if let Some(scope) = case_scope.last_mut() {
                            scope.extend(match_pattern_bindings(&pattern));
                        }
                        (pattern, self.rename_expression(body, &case_scope))
                    })
                    .collect();
                let default = Box::new(self.rename_expression(*default, scope_stack));
                Expression::Match {
                    value: Box::new(value),
                    cases,
                    default,
                    data_type: self.rename_data_type(data_type, scope_stack),
                }
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
            } => Expression::EnumVariantPath {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
            } => Expression::EnumVariant {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                payloads: payloads
                    .into_iter()
                    .map(|payload| self.rename_expression(payload, scope_stack))
                    .collect(),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Literal(literal) => Expression::Literal(match literal {
                Literal::List(elements) => Literal::List(
                    elements
                        .into_iter()
                        .map(|element| self.rename_expression(element, scope_stack))
                        .collect(),
                ),
                Literal::Dict(entries) => Literal::Dict(
                    entries
                        .into_iter()
                        .map(|((key, value), data_type)| {
                            (
                                (
                                    self.rename_expression(key, scope_stack),
                                    self.rename_expression(value, scope_stack),
                                ),
                                self.rename_data_type(data_type, scope_stack),
                            )
                        })
                        .collect(),
                ),
                Literal::Tuple(elements) => Literal::Tuple(
                    elements
                        .into_iter()
                        .map(|element| self.rename_expression(element, scope_stack))
                        .collect(),
                ),
                other => other,
            }),
        }
    }

    fn rename_query_op(
        &self,
        op: crate::parser::ast::QueryOp,
        scope_stack: &[HashSet<String>],
    ) -> crate::parser::ast::QueryOp {
        match op {
            crate::parser::ast::QueryOp::Insert { assigns } => {
                crate::parser::ast::QueryOp::Insert {
                    assigns: assigns
                        .into_iter()
                        .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                        .collect(),
                }
            }
            crate::parser::ast::QueryOp::Update { condition, assigns } => {
                crate::parser::ast::QueryOp::Update {
                    condition: self.rename_expression(condition, scope_stack),
                    assigns: assigns
                        .into_iter()
                        .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                        .collect(),
                }
            }
            crate::parser::ast::QueryOp::Delete { condition } => {
                crate::parser::ast::QueryOp::Delete {
                    condition: self.rename_expression(condition, scope_stack),
                }
            }
            crate::parser::ast::QueryOp::Get(mut get) => {
                get.condition = self.rename_expression(get.condition, scope_stack);
                get.body = self.rename_statement_block(get.body, &mut scope_stack.to_vec());
                crate::parser::ast::QueryOp::Get(get)
            }
            other => other,
        }
    }

    fn rename_enum_variant(
        &self,
        mut variant: EnumVariantDef,
        enum_name: &str,
        scope_stack: &[HashSet<String>],
    ) -> EnumVariantDef {
        variant.enum_name = enum_name.to_string();
        variant.data_types = variant
            .data_types
            .into_iter()
            .map(|data_type| self.rename_data_type(data_type, scope_stack))
            .collect();
        variant
    }
}

fn is_shadowed(scope_stack: &[HashSet<String>], name: &str) -> bool {
    scope_stack.iter().rev().any(|scope| scope.contains(name))
}

fn match_pattern_bindings(pattern: &Expression) -> Vec<String> {
    let mut bindings = Vec::new();
    match pattern {
        Expression::EnumVariant { payloads, .. } => {
            for payload in payloads {
                if let Expression::Identifier(Identifier { name, .. }) = payload {
                    bindings.push(name.clone());
                }
            }
        }
        Expression::Call { name, args, .. } if name == "__match_guard" || name == "__match_or" => {
            if let Some(inner) = args.first() {
                bindings.extend(match_pattern_bindings(inner));
            }
        }
        _ => {}
    }
    bindings
}

fn statement_bindings(statement: &Statement) -> Vec<String> {
    let mut bindings = Vec::new();
    match statement {
        Statement::Let { name, .. }
        | Statement::Function { name, .. }
        | Statement::Type { name, .. }
        | Statement::Skill { name, .. }
        | Statement::Module { name, .. }
        | Statement::Enum { name, .. }
        | Statement::ExternLib { name, .. }
        | Statement::ExternFunction { name, .. } => bindings.push(name.clone()),
        Statement::For {
            variable, index, ..
        } => {
            bindings.push(variable.clone());
            if let Some(index) = index {
                bindings.push(index.clone());
            }
        }
        Statement::Find { variable, .. }
        | Statement::Move {
            target: variable, ..
        } => bindings.push(variable.clone()),
        _ => {}
    }
    bindings
}

fn select_imported_statements(
    statements: &[ExpandedStatement],
    items: Option<&[String]>,
    import_path: &Path,
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
                        if statement_export_name(&statement.statement) == Some(candidate_name)
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

    Ok(statements
        .iter()
        .filter(|statement| statement_export_name(&statement.statement).is_some())
        .cloned()
        .collect())
}

fn read_source_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
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

impl ResolvedFile {
    fn from_cached(cached: CachedParsedFile, source: String) -> Self {
        drop(source);
        Self {
            hash: cached.hash,
            program: cached.program,
            exports: cached.exports,
        }
    }
}
