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
            message: format!("Could not read '{}': {}", manifest_path.display(), err),
        })
    })?;

    let manifest: MireManifest = toml::from_str(&raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
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
            message: format!("Could not serialize Mire.lock: {}", err),
        })
    })?;

    fs::write(project_lock_path(cwd), raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
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
            message: format!("Could not serialize manifest: {}", err),
        })
    })?;
    fs::write(path, raw).map_err(|err| {
        MireError::new(ErrorKind::Runtime {
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
    exports.get(name).map(|relative| {
        let candidate = package_root.join(relative);
        if candidate.extension().is_some() {
            candidate
        } else {
            candidate.join("mod.mire")
        }
    })
}
