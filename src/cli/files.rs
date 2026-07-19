use mire::MireError;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn walkdir(dir: &Path, _pattern: &str) -> Result<Vec<PathBuf>, MireError> {
    let mut results = Vec::new();
    if !dir.is_dir() {
        return Ok(results);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(ext) = path.extension()
                && ext == "mire"
            {
                results.push(path);
            }
        }
    }
    Ok(results)
}

pub(crate) fn runtime_msg(message: &str) -> MireError {
    MireError::runtime(message.to_string())
}

pub(crate) fn runtime_err(err: std::io::Error) -> MireError {
    MireError::runtime(err.to_string())
}

pub(crate) fn set_owl_home_env(path: Option<&PathBuf>) {
    if let Some(path) = path {
        unsafe {
            std::env::set_var("MIRE_OWL_HOME", path);
        }
    }
}
