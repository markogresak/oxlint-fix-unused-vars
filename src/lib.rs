mod options;
mod parse;
mod paths;
mod remove;
mod run;
mod vendor;

pub use options::{
    FileResult, NoUnusedVarsConfig, Removal, RemoveUnusedVarsOptions, RemoveUnusedVarsResult,
    RunError, WriteOption,
};
pub use run::remove_unused_vars;
pub use vendor::no_unused_vars::{
    find_unused_bindings, ArgsOption, CaughtErrors, IgnorePattern, NoUnusedVarsOptions,
    UnusedBinding, UnusedKind, VarsOption,
};
