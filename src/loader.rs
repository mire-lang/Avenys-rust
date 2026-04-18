use crate::avens::find_project_root;
use crate::error::{ErrorKind, MireError, Result};
use crate::incremental::{
    CachedImport, CachedParsedFile, IncrementalCache, LoadedFile, LoadedProgram, source_hash,
    statement_export_name,
};
use crate::parser::ast::Statement;
use crate::parser::{Program, parse};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn load_program_from_file(path: &Path) -> Result<Program> {
    Ok(load_program_with_metadata(path)?.program)
}

pub fn load_program_with_metadata(path: &Path) -> Result<LoadedProgram> {
    let canonical = path.canonicalize().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not resolve '{}': {}", path.display(), err),
        })
    })?;

    let Some(project_root) =
        find_project_root(canonical.parent().unwrap_or_else(|| Path::new(".")))
    else {
        return load_shallow_program(&canonical);
    };

    let mut resolver = ImportResolver::new(project_root, IncrementalCache::load_for(&canonical));
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

fn load_shallow_program(path: &Path) -> Result<LoadedProgram> {
    let source = read_source_file(path)?;
    let hash = source_hash(&source);
    let program = parse(&source).map_err(|err| {
        err.with_source(source.clone())
            .with_filename(path.display().to_string())
    })?;
    if contains_local_import(&program.statements) {
        return Err(MireError::new(ErrorKind::Runtime {
            message: format!(
                "Local import statements require a Mire project root with project.toml: '{}'",
                path.display()
            ),
        }));
    }
    let mut files = HashMap::new();
    files.insert(
        path.to_path_buf(),
        LoadedFile {
            hash,
            direct_dependencies: Vec::new(),
        },
    );
    let statement_origins = vec![path.to_path_buf(); program.statements.len()];
    let mut sources = HashMap::new();
    sources.insert(path.to_path_buf(), source);
    Ok(LoadedProgram {
        program,
        files,
        statement_origins,
        sources,
    })
}

struct ImportResolver {
    project_root: PathBuf,
    cache: IncrementalCache,
    expanded_cache: HashMap<PathBuf, Vec<ExpandedStatement>>,
    active_stack: HashSet<PathBuf>,
    files: HashMap<PathBuf, LoadedFile>,
    sources: HashMap<PathBuf, String>,
}

impl ImportResolver {
    fn new(project_root: PathBuf, cache: IncrementalCache) -> Self {
        Self {
            project_root,
            cache,
            expanded_cache: HashMap::new(),
            active_stack: HashSet::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
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
                message: format!("Cyclic local import detected at '{}'", canonical.display()),
            }));
        }

        let parsed = self.load_or_parse_file(&canonical)?;
        let mut expanded = Vec::new();
        let mut direct_dependencies = Vec::new();
        for statement in parsed.program.statements {
            match statement {
                Statement::Use {
                    path,
                    alias,
                    items,
                    is_local,
                } if is_local => {
                    if alias.is_some() {
                        self.active_stack.remove(&canonical);
                        return Err(MireError::new(ErrorKind::Runtime {
                            message: "Local import statements do not support aliasing".to_string(),
                        }));
                    }
                    let imported_path = match parsed
                        .local_imports
                        .iter()
                        .find(|import| import.raw_path == path && import.items == items)
                    {
                        Some(import) => import.resolved_path.clone(),
                        None => {
                            self.active_stack.remove(&canonical);
                            return Err(MireError::new(ErrorKind::Runtime {
                                message: format!(
                                    "Incremental cache is missing local import metadata for '{}'",
                                    path
                                ),
                            }));
                        }
                    };
                    let imported = if items.is_some() {
                        self.load_selected_imports(&imported_path, items.as_deref())?
                    } else {
                        self.load_file(&imported_path)?
                    };
                    direct_dependencies.push(imported_path.clone());
                    expanded.extend(select_imported_statements(
                        &imported,
                        items.as_deref(),
                        &imported_path,
                    )?);
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
        self.expanded_cache.insert(canonical, expanded.clone());
        Ok(expanded)
    }

    fn load_or_parse_file(&mut self, path: &Path) -> Result<ResolvedFile> {
        let source = read_source_file(path)?;
        self.sources.insert(path.to_path_buf(), source.clone());
        let hash = source_hash(&source);
        if let Some(cached) = self.cache.cached_file(path, hash) {
            return Ok(ResolvedFile::from_cached(cached, source));
        }

        let program = parse(&source).map_err(|err| {
            err.with_source(source.clone())
                .with_filename(path.display().to_string())
        })?;
        let mut local_imports = Vec::new();
        for statement in &program.statements {
            if let Statement::Use {
                path,
                items,
                is_local,
                ..
            } = statement
            {
                if *is_local {
                    local_imports.push(CachedImport {
                        raw_path: path.clone(),
                        resolved_path: self.resolve_local_import(path)?,
                        items: items.clone(),
                    });
                }
            }
        }
        let cached = CachedParsedFile {
            hash,
            exports: program
                .statements
                .iter()
                .filter_map(statement_export_name)
                .map(ToString::to_string)
                .collect(),
            local_imports,
            program: program.clone(),
        };
        self.cache.store_file(path, cached.clone());
        Ok(ResolvedFile::from_cached(cached, source))
    }

    fn load_selected_imports(
        &mut self,
        path: &Path,
        items: Option<&[String]>,
    ) -> Result<Vec<ExpandedStatement>> {
        let parsed = self.load_or_parse_file(path)?;
        if !parsed.local_imports.is_empty() {
            return self.load_file(path);
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
        select_imported_statements(
            &expanded,
            items,
            path,
        )
    }

    fn resolve_local_import(&self, raw_path: &str) -> Result<PathBuf> {
        if !raw_path.starts_with("./") {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!("Local import '{}' must start with './'", raw_path),
            }));
        }

        let relative = &raw_path[2..];
        let mut candidate = self.project_root.join(relative);
        if candidate.extension().is_none() {
            candidate.set_extension("mire");
        }

        let canonical = candidate.canonicalize().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not resolve local import '{}': {}", raw_path, err),
            })
        })?;

        if !canonical.starts_with(&self.project_root) {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!(
                    "Local import '{}' escapes the project root '{}'",
                    raw_path,
                    self.project_root.display()
                ),
            }));
        }

        Ok(canonical)
    }
}

fn select_imported_statements(
    statements: &[ExpandedStatement],
    items: Option<&[String]>,
    import_path: &Path,
) -> Result<Vec<ExpandedStatement>> {
    if let Some(items) = items {
        let mut selected = Vec::new();
        for item in items {
            let statement = statements
                .iter()
                .find(|statement| statement_export_name(&statement.statement) == Some(item.as_str()))
                .cloned()
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Local import '{}' does not export '{}'",
                            import_path.display(),
                            item
                        ),
                    })
                })?;
            selected.push(statement);
        }
        return Ok(selected);
    }

    Ok(statements
        .iter()
        .filter(|statement| statement_export_name(&statement.statement).is_some())
        .cloned()
        .collect())
}

fn contains_local_import(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Use { is_local, .. } => *is_local,
        _ => false,
    })
}

fn read_source_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not read '{}': {}", path.display(), err),
        })
    })?;
    let capacity = file
        .metadata()
        .ok()
        .and_then(|metadata| usize::try_from(metadata.len()).ok())
        .unwrap_or(0);
    let mut source = String::with_capacity(capacity.saturating_add(1));
    file.read_to_string(&mut source).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not read '{}': {}", path.display(), err),
        })
    })?;
    Ok(source)
}

struct ResolvedFile {
    hash: u64,
    program: Program,
    local_imports: Vec<CachedImport>,
}

#[derive(Clone)]
struct ExpandedStatement {
    statement: Statement,
    origin: PathBuf,
}

impl ResolvedFile {
    fn from_cached(cached: CachedParsedFile, _source: String) -> Self {
        Self {
            hash: cached.hash,
            program: cached.program,
            local_imports: cached.local_imports,
        }
    }
}