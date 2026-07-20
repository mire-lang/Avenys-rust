use crate::error::diagnostic::{Diagnostic, DiagnosticCode, WarningFilter};
use crate::incremental::CacheOverrides;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BuildMode {
    #[default]
    Debug,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OptLevel {
    #[default]
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

impl OptLevel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "0" | "O0" | "o0" => Some(Self::O0),
            "1" | "O1" | "o1" => Some(Self::O1),
            "2" | "O2" | "o2" => Some(Self::O2),
            "3" | "O3" | "o3" => Some(Self::O3),
            "s" | "S" | "os" | "Os" => Some(Self::Os),
            "z" | "Z" | "oz" | "Oz" => Some(Self::Oz),
            _ => None,
        }
    }

    pub fn as_opt_flag(self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::O2 => "-O2",
            Self::O3 => "-O3",
            Self::Os => "-Os",
            Self::Oz => "-Oz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ImportMode {
    #[default]
    Reachable,
}

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    pub mode: BuildMode,
    pub opt_level: OptLevel,
    pub debug_dump: bool,
    pub output: Option<PathBuf>,
    pub emit_binary: bool,
    pub persist_ir: bool,
    pub import_mode: ImportMode,
    pub cache: CacheOverrides,
    pub warning_filter: WarningFilter,
    pub deny_warnings: HashSet<DiagnosticCode>,
    pub module_paths: Vec<PathBuf>,
    pub test_mode: bool,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub binary_path: PathBuf,
    pub ir_path: Option<PathBuf>,
    pub optimized_ir_path: Option<PathBuf>,
    pub used_optimizations: bool,
    pub warnings: Vec<String>,
    pub warnings_raw: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireManifest {
    #[serde(alias = "package")]
    #[serde(alias = "owl")]
    pub project: MireProject,
    #[serde(default)]
    pub cache: Option<MireCacheConfig>,
    #[serde(default)]
    #[serde(alias = "imports")]
    pub dependencies: MireDependencies,
    #[serde(default)]
    pub exports: Option<ExportsSection>,
    #[serde(default)]
    pub bootstrap: Option<BootstrapConfig>,
    #[serde(default)]
    pub builtins: Option<MireBuiltins>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportsSection {
    #[serde(flatten)]
    pub entries: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default = "default_std_package")]
    pub std_package: String,
    pub std_entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MireBuiltins {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allow: Vec<String>,
}

fn default_std_package() -> String {
    "kioto".to_string()
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            std_package: default_std_package(),
            std_entry: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MireDependencies {
    #[serde(flatten)]
    pub entries: HashMap<String, MireDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MireDependency {
    Simple { version: String },
    WithPath { version: String, path: String },
    PathOnly { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireProject {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "mod.mire".to_string()
}

impl Default for MireProject {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            entry: default_entry(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MireCacheConfig {
    pub max_units: Option<usize>,
    pub analysis_cache: Option<bool>,
    pub compression: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLock {
    #[serde(alias = "package")]
    pub project: MireLockProject,
    pub build: MireLockBuild,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLockProject {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MireLockBuild {
    pub llvm_version: String,
    pub profile: String,
    pub opt_level: String,
}
