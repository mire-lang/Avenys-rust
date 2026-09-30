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
    pub c_defs: CDefs,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    #[serde(default)]
    pub macros: Option<MireMacros>,
    #[serde(default)]
    pub security: Option<SecurityConfig>,
    #[serde(default)]
    pub paths: Option<MirePaths>,
    #[serde(default)]
    pub c: CDefs,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirePaths {
    #[serde(default)]
    pub bin: Option<PathBuf>,
    #[serde(default)]
    pub cache: Option<PathBuf>,
    #[serde(default)]
    pub generated: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CDefs {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub include: Vec<PathBuf>,
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub libs: Vec<String>,
    /// Runtime tier: minimal (default) | full | none (freestanding).
    #[serde(default)]
    pub runtime: RuntimeTier,
    /// LLVM target triple. Avenys 4 defaults to the supported Linux host ABI;
    /// Owl may replace it with a target selected by its toolchain resolver.
    #[serde(default)]
    pub target: Option<String>,
    /// Skip C runtime startup files (crt1.o, crti.o, crtn.o) for true
    /// no-libc freestanding binaries. Requires `runtime = "none"` and
    /// a user-provided entry point (e.g. `_start`).
    #[serde(default)]
    pub nostartfiles: bool,
    /// Skip standard library (libc, libgcc, etc.) for true no-libc freestanding.
    /// Requires `runtime = "none"` and `nostartfiles = true`.
    #[serde(default)]
    pub nostdlib: bool,
    /// Output artifact: executable, static archive, or shared object.
    #[serde(default)]
    pub artifact: LibType,
}

impl Default for CDefs {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            include: Vec::new(),
            cflags: Vec::new(),
            libs: Vec::new(),
            runtime: RuntimeTier::default(),
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            nostartfiles: false,
            nostdlib: false,
            artifact: LibType::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportsSection {
    #[serde(flatten)]
    pub entries: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MireMacros {
    #[serde(flatten)]
    pub entries: HashMap<String, String>,
}

/// Controls how much of the Mire runtime is linked into the final binary.
/// - `minimal`: only symbols actually referenced in the program are declared and linked.
/// - `none`:  no Mire runtime at all; the program must provide its own panic handler
///   and any PAL symbols it needs.  PAL declarations are still emitted when
///   the program actually calls PAL functions (PAL is separate from the runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum RuntimeTier {
    Full,
    #[default]
    Minimal,
    None,
}

impl RuntimeTier {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "minimal" => Some(Self::Minimal),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Output artifact selected by `--artifact` or Owl's normalized config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LibType {
    #[default]
    Bin,
    Static,
    Shared,
}

impl LibType {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "bin" => Some(Self::Bin),
            "static" | "staticlib" | "static-lib" => Some(Self::Static),
            "shared" | "cdylib" | "dylib" => Some(Self::Shared),
            _ => None,
        }
    }
}

impl<'de> serde::Deserialize<'de> for LibType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid library type: {} (expected bin, static, or shared)",
                s
            ))
        })
    }
}


impl serde::Serialize for RuntimeTier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            RuntimeTier::Full => "full",
            RuntimeTier::Minimal => "minimal",
            RuntimeTier::None => "none",
        })
    }
}

impl<'de> serde::Deserialize<'de> for RuntimeTier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "full" => Ok(RuntimeTier::Full),
            "minimal" => Ok(RuntimeTier::Minimal),
            "none" => Ok(RuntimeTier::None),
            _ => Err(serde::de::Error::custom(format!(
                "invalid runtime tier: {}",
                s
            ))),
        }
    }
}

/// Security trust tier for code execution.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum TrustTier {
    #[default]
    Code,
    Macros,
    Ffi,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub mode: SecurityMode,
    #[serde(default)]
    #[serde(alias = "allow-unsafe")]
    pub unsafe_allowed: bool,
    #[serde(default)]
    pub asm_allowed: bool,
    #[serde(default)]
    pub externs: Vec<String>,
    #[serde(default)]
    #[serde(alias = "extern-libs")]
    pub extern_libs: Vec<String>,
    #[serde(default)]
    pub macros: Vec<String>,
    #[serde(default)]
    pub deps: HashMap<String, TrustTier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecurityMode {
    #[default]
    Open,
    Strict,
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
    pub blob_checksum: Option<bool>,
}
