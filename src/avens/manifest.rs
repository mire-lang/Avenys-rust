use super::*;
use std::collections::HashMap;

/// Normalized project configuration emitted by Owl. Avenys accepts this file
/// as a closed compiler contract and does not resolve the originating
/// `owl.toml` or dependency graph when it is supplied with `--config`.
///
/// Dependency management is Owl's job, not the compiler's. Avenys never reads,
/// writes, updates or validates a lockfile (`owl.lock` or any other), never
/// resolves version constraints, and never picks a dependency version. It is
/// handed source code plus this normalized config, it keeps its own build cache
/// under `bin/.cache`, and it compiles. Anything that needs to know *which*
/// version of a library to use has already been decided upstream by Owl.
#[derive(Debug, Clone, serde::Deserialize)]
struct MireConfigFile {
    #[serde(default)]
    project: MireConfigProject,
    #[serde(default)]
    build: MireConfigBuild,
    #[serde(default)]
    cfg: MireConfigCfg,
    #[serde(default)]
    security: Option<SecurityConfig>,
    #[serde(default)]
    paths: MirePaths,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct MireConfigProject {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    entry: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MireConfigBuild {
    #[serde(default = "default_config_runtime")]
    runtime: RuntimeTier,
    #[serde(default = "default_config_target")]
    target: Option<String>,
    #[serde(default = "default_config_artifact")]
    artifact: LibType,
}

impl Default for MireConfigBuild {
    fn default() -> Self {
        Self {
            runtime: default_config_runtime(),
            target: default_config_target(),
            artifact: default_config_artifact(),
        }
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct MireConfigCfg {
    #[serde(default)]
    libs: Vec<String>,
    #[serde(default, alias = "link_dirs")]
    link_dirs: Vec<String>,
    #[serde(default)]
    cflags: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
}

fn default_config_runtime() -> RuntimeTier {
    RuntimeTier::Minimal
}

fn default_config_target() -> Option<String> {
    Some("x86_64-unknown-linux-gnu".to_string())
}

fn default_config_artifact() -> LibType {
    LibType::Bin
}

/// Load Owl's normalized `mire-config.toml` without consulting project
/// manifests, registries, lockfiles, or dependency metadata.
pub fn load_config_file(config_path: &Path) -> Result<MireManifest> {
    let raw = fs::read_to_string(config_path).map_err(|err| {
        MireError::runtime(format!(
            "Could not read compiler config '{}': {}",
            config_path.display(),
            err
        ))
    })?;
    let config: MireConfigFile = toml::from_str(&raw).map_err(|err| {
        MireError::runtime(format!(
            "Invalid compiler config '{}': {}",
            config_path.display(),
            err
        ))
    })?;
    let mut c = CDefs {
        runtime: config.build.runtime,
        target: config.build.target,
        artifact: config.build.artifact,
        libs: config.cfg.libs,
        sources: config.cfg.sources,
        cflags: config
            .cfg
            .link_dirs
            .into_iter()
            .map(|dir| format!("-L{dir}"))
            .collect(),
        ..CDefs::default()
    };
    c.cflags.extend(config.cfg.cflags);
    let config_root = config_path.parent().unwrap_or_else(|| Path::new("."));
    let default_entry = if config_root.join("mod.mire").exists() {
        "mod.mire"
    } else {
        "mod.mr"
    };
    Ok(MireManifest {
        project: MireProject {
            name: config.project.name,
            version: config.project.version,
            entry: if config.project.entry.is_empty() {
                default_entry.to_string()
            } else {
                config.project.entry
            },
        },
        c,
        security: config.security,
        paths: Some(config.paths),
        ..MireManifest::default()
    })
}

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

/// Directory names that are shared scratch space rather than a project root.
///
/// A manifest sitting directly in one of these is not owned by the build that
/// happens to be walking upwards: any project created under a temp directory
/// would otherwise "inherit" it and have its own manifest silently ignored, and
/// a malformed scratch copy would fail unrelated builds. Subdirectories of a
/// temp root are unaffected — a real project there brings its own manifest, so
/// the walk returns before reaching the root itself.
fn is_shared_temp_root(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str());
    let is_tmpdir = std::env::var("TMPDIR")
        .ok()
        .map(|t| Path::new(&t) == path)
        .unwrap_or(false);
    matches!(name, Some("tmp") | Some("temp")) || is_tmpdir || path == Path::new("/tmp")
}

pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if (path.join("owl.toml").exists() || path.join("Mire.toml").exists())
            && !is_shared_temp_root(path)
        {
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
            } else if canonical.join("mod.mire").exists() {
                Some(canonical.join("mod.mire"))
            } else {
                Some(canonical.join("mod.mr"))
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
