use std::path::PathBuf;

use crate::{NoUnusedVarsOptions, VarsOption};

#[derive(Debug, Clone)]
pub enum NoUnusedVarsConfig {
    Vars(VarsOption),
    Options(NoUnusedVarsOptions),
}

impl Default for NoUnusedVarsConfig {
    fn default() -> Self {
        Self::Vars(VarsOption::All)
    }
}

impl NoUnusedVarsConfig {
    pub(crate) fn resolve(&self) -> NoUnusedVarsOptions {
        match self {
            Self::Vars(vars) => NoUnusedVarsOptions {
                vars: vars.clone(),
                ..NoUnusedVarsOptions::default()
            },
            Self::Options(options) => options.clone(),
        }
    }
}

/// Controls whether files are written, and optionally how many write passes to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOption {
    /// Dry run; do not write files.
    Disabled,
    /// Write changed files. When `passes` is `Some(n)`, re-run up to `n` times
    /// while any files are written, tagging each result with its 1-based pass.
    Enabled { passes: Option<usize> },
}

impl Default for WriteOption {
    fn default() -> Self {
        Self::Disabled
    }
}

impl From<bool> for WriteOption {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled { passes: None }
        } else {
            Self::Disabled
        }
    }
}

impl WriteOption {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    /// Maximum write passes. `None` means a single pass without pass tagging.
    pub fn max_passes(&self) -> Option<usize> {
        match self {
            Self::Disabled | Self::Enabled { passes: None } => None,
            Self::Enabled { passes: Some(passes) } => Some(*passes),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedVarsOptions {
    pub root: PathBuf,
    pub path: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub no_unused_vars_config: NoUnusedVarsConfig,
    pub write: WriteOption,
    pub include_removals: bool,
    pub threads: usize,
}

impl Default for RemoveUnusedVarsOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            path: Vec::new(),
            ignore_patterns: Vec::new(),
            no_unused_vars_config: NoUnusedVarsConfig::default(),
            write: WriteOption::Disabled,
            include_removals: false,
            threads: std::thread::available_parallelism().map_or(1, usize::from),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedVarsResult {
    pub results: Vec<FileResult>,
    pub errors: Vec<RunError>,
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: PathBuf,
    pub updated: Option<String>,
    pub removals: Option<Vec<Removal>>,
    /// 1-based pass index when `write: { enabled: true, passes }` is used.
    pub pass: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct RunError {
    pub path: Option<PathBuf>,
    pub message: String,
}
