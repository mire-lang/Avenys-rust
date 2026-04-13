use crate::avens::find_project_root;
use crate::error::{ErrorKind, MireError, Result};
use crate::parser::ast::Statement;
use crate::parser::{parse, Program};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn load_program_from_file(path: &Path) -> Result<Program> {
    let canonical = path.canonicalize().map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            message: format!("Could not resolve '{}': {}", path.display(), err),
        })
    })?;

    let Some(project_root) = find_project_root(canonical.parent().unwrap_or_else(|| Path::new("."))) else {
        return load_shallow_program(&canonical);
    };

    let mut resolver = ImportResolver::new(project_root);
    let statements = resolver.load_file(&canonical)?;
    Ok(Program { statements })
}

fn load_shallow_program(path: &Path) -> Result<Program> {
    let source = read_source_file(path)?;
    let program =
        parse(&source).map_err(|err| err.with_source(source).with_filename(path.display().to_string()))?;
    if contains_local_import(&program.statements) {
        return Err(MireError::new(ErrorKind::Runtime {
            message: format!(
                "Local import statements require a Mire project root with project.toml: '{}'",
                path.display()
            ),
        }));
    }
    Ok(program)
}

struct ImportResolver {
    project_root: PathBuf,
    cache: HashMap<PathBuf, Vec<Statement>>,
    active_stack: HashSet<PathBuf>,
}

impl ImportResolver {
    fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            cache: HashMap::new(),
            active_stack: HashSet::new(),
        }
    }

    fn load_file(&mut self, path: &Path) -> Result<Vec<Statement>> {
        let canonical = path.canonicalize().map_err(|err| {
            MireError::new(ErrorKind::Runtime {
                message: format!("Could not resolve '{}': {}", path.display(), err),
            })
        })?;

        if let Some(cached) = self.cache.get(&canonical) {
            return Ok(cached.clone());
        }

        if !self.active_stack.insert(canonical.clone()) {
            return Err(MireError::new(ErrorKind::Runtime {
                message: format!("Cyclic local import detected at '{}'", canonical.display()),
            }));
        }

        let source = read_source_file(&canonical)?;
        let program = parse(&source)
            .map_err(|err| err.with_source(source).with_filename(canonical.display().to_string()))?;
        let mut expanded = Vec::new();
        for statement in program.statements {
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
                    let imported_path = self.resolve_local_import(&path)?;
                    let imported = self.load_file(&imported_path)?;
                    expanded.extend(select_imported_statements(&imported, items.as_deref(), &path)?);
                }
                other => expanded.push(other),
            }
        }

        self.active_stack.remove(&canonical);
        self.cache.insert(canonical, expanded.clone());
        Ok(expanded)
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
    statements: &[Statement],
    items: Option<&[String]>,
    import_path: &str,
) -> Result<Vec<Statement>> {
    if let Some(items) = items {
        let mut selected = Vec::new();
        for item in items {
            let statement = statements
                .iter()
                .find(|statement| statement_export_name(statement) == Some(item.as_str()))
                .cloned()
                .ok_or_else(|| {
                    MireError::new(ErrorKind::Runtime {
                        message: format!(
                            "Local import '{}' does not export '{}'",
                            import_path, item
                        ),
                    })
                })?;
            selected.push(statement);
        }
        return Ok(selected);
    }

    Ok(statements
        .iter()
        .filter(|statement| statement_export_name(statement).is_some())
        .cloned()
        .collect())
}

fn statement_export_name(statement: &Statement) -> Option<&str> {
    match statement {
        Statement::Let { name, .. }
        | Statement::Function { name, .. }
        | Statement::Type { name, .. }
        | Statement::Class { name, .. }
        | Statement::Trait { name, .. }
        | Statement::Skill { name, .. }
        | Statement::Module { name, .. }
        | Statement::Enum { name, .. }
        | Statement::ExternLib { name, .. }
        | Statement::ExternFunction { name, .. } => Some(name.as_str()),
        _ => None,
    }
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