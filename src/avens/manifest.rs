use super::toolchain::llvm_version;
use super::*;
use std::collections::HashMap;

pub fn load_project_manifest(cwd: &Path) -> Result<Option<MireManifest>> {
    let manifest_path = project_manifest_path(cwd);
    if !manifest_path.exists() {
        let legacy = cwd.join("Mire.toml");
        if !legacy.exists() {
            return Ok(None);
        }
        return load_manifest_file(&legacy);
    }

    load_manifest_file(&manifest_path)
}

fn load_manifest_file(manifest_path: &Path) -> Result<Option<MireManifest>> {
    if !manifest_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(manifest_path).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not read '{}': {}", manifest_path.display(), err),
        })
    })?;

    let manifest: MireManifest = toml::from_str(&raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Invalid Mire.toml: {}", err),
        })
    })?;

    Ok(Some(manifest))
}

pub fn write_lock_file(cwd: &Path, manifest: &MireManifest, mode: BuildMode) -> Result<()> {
    let llvm_version = llvm_version()?;
    let lock = MireLock {
        project: MireLockProject {
            name: manifest.project.name.clone(),
            version: manifest.project.version.clone(),
        },
        build: MireLockBuild {
            llvm_version,
            profile: match mode {
                BuildMode::Debug => "debug".to_string(),
                BuildMode::Release => "release".to_string(),
            },
            opt_level: match mode {
                BuildMode::Debug => "0".to_string(),
                BuildMode::Release => "3".to_string(),
            },
        },
    };

    let raw = toml::to_string_pretty(&lock).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not serialize Mire.lock: {}", err),
        })
    })?;

    fs::write(project_lock_path(cwd), raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not write project.lock: {}", err),
        })
    })?;

    Ok(())
}

pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join("owl.toml").exists() || path.join("Mire.toml").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

pub fn project_manifest_path(cwd: &Path) -> PathBuf {
    if cwd.join("owl.toml").exists() {
        return cwd.join("owl.toml");
    }
    if cwd.join("Mire.toml").exists() {
        return cwd.join("Mire.toml");
    }
    cwd.join("owl.toml")
}

pub fn project_lock_path(cwd: &Path) -> PathBuf {
    if cwd.join("owl.lock").exists() {
        return cwd.join("owl.lock");
    }
    if cwd.join("project.lock").exists() {
        return cwd.join("project.lock");
    }
    cwd.join("Mire.lock")
}

pub fn write_manifest(manifest: &MireManifest, path: &Path) -> Result<()> {
    let raw = toml::to_string_pretty(manifest).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not serialize manifest: {}", err),
        })
    })?;
    fs::write(path, raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
            span: crate::error::Span::unknown(),
            message: format!("Could not write manifest '{}': {}", path.display(), err),
        })
    })?;
    Ok(())
}

pub fn load_manifest_dependencies(cwd: &Path) -> Result<HashMap<String, MireDependency>> {
    match load_project_manifest(cwd) {
        Ok(Some(manifest)) => Ok(manifest.dependencies.entries),
        Ok(None) => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
}

pub fn load_exports(cwd: &Path) -> Result<HashMap<String, String>> {
    match load_project_manifest(cwd) {
        Ok(Some(manifest)) => Ok(manifest.exports.map(|e| e.entries).unwrap_or_default()),
        Ok(None) => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
}

pub fn resolve_export_path(
    exports: &HashMap<String, String>,
    package_root: &Path,
    name: &str,
) -> Option<PathBuf> {
    exports.get(name).and_then(|relative| {
        let candidate = package_root.join(relative);
        let canonical = candidate.canonicalize().ok()?;
        let canonical_root = package_root.canonicalize().ok()?;
        if canonical.starts_with(canonical_root.as_path()) {
            if canonical.extension().is_some() {
                Some(canonical)
            } else {
                Some(canonical.join("mod.mire"))
            }
        } else {
            None
        }
    })
}

/// Result of validating a manifest `entry` path against the package root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryContainment {
    /// The entry exists and canonicalizes to a path inside the package root.
    Contained,
    /// The entry does not exist (a later load will surface the error).
    DoesNotExist,
    /// The entry is absolute or uses `..` to escape the package root.
    EscapesRoot,
}

/// Check that a manifest `entry` path stays inside the package root.
///
/// Rejects absolute paths outside the root and relative paths containing `..`
/// that resolve outside it (path traversal; see docs/SECURITY.md item 5).
/// Non-existent entries are not rejected here — the load path reports them.
pub fn check_entry_containment(package_root: &Path, entry: &str) -> EntryContainment {
    let joined = package_root.join(entry);
    let canonical_root = package_root.canonicalize().ok();
    match joined.canonicalize() {
        Ok(canonical) => match canonical_root {
            Some(root) if canonical.starts_with(root.as_path()) => EntryContainment::Contained,
            _ => EntryContainment::EscapesRoot,
        },
        Err(_) => {
            if joined.is_absolute()
                && !canonical_root
                    .as_ref()
                    .is_some_and(|root| joined.starts_with(root.as_path()))
            {
                EntryContainment::EscapesRoot
            } else {
                EntryContainment::DoesNotExist
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(label: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("mire_{label}_{ts}"))
    }

    #[test]
    fn entry_containment_accepts_entry_inside_root() {
        let root = unique_dir("entry_ok");
        fs::create_dir_all(root.join("src")).expect("dirs");
        fs::write(root.join("src/main.mire"), "").expect("file");
        assert_eq!(
            check_entry_containment(&root, "src/main.mire"),
            EntryContainment::Contained
        );
        // Missing entry is not an escape — the load path reports it.
        assert_eq!(
            check_entry_containment(&root, "mod.mire"),
            EntryContainment::DoesNotExist
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn entry_containment_rejects_absolute_escape() {
        let root = unique_dir("entry_abs");
        fs::create_dir_all(&root).expect("dirs");
        assert_eq!(
            check_entry_containment(&root, "/etc/passwd"),
            EntryContainment::EscapesRoot
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn entry_containment_rejects_dotdot_escape() {
        let root = unique_dir("entry_dotdot");
        let parent = root.parent().expect("parent");
        fs::create_dir_all(&root).expect("dirs");
        let escape_file = parent.join("escape_target.mire");
        fs::write(&escape_file, "").expect("escape target");
        assert_eq!(
            check_entry_containment(&root, "../escape_target.mire"),
            EntryContainment::EscapesRoot
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&escape_file);
    }
}
