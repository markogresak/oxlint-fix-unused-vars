use std::path::PathBuf;

use napi::{Either, Error, Result, Status};
use napi_derive::napi;
use oxlint_fix_unused_vars::{
    remove_unused_vars, ArgsOption, CaughtErrors, IgnorePattern, NoUnusedVarsConfig,
    NoUnusedVarsOptions, RemoveUnusedVarsOptions, VarsOption, WriteOption,
};

#[napi(object)]
pub struct JsNoUnusedVarsOptions {
    pub vars: Option<String>,
    pub vars_ignore_pattern: Option<String>,
    pub args: Option<String>,
    pub args_ignore_pattern: Option<String>,
    pub ignore_rest_siblings: Option<bool>,
    pub caught_errors: Option<String>,
    pub caught_errors_ignore_pattern: Option<String>,
    pub destructured_array_ignore_pattern: Option<String>,
    pub ignore_class_with_static_init_block: Option<bool>,
    pub ignore_using_declarations: Option<bool>,
    pub report_vars_only_used_as_types: Option<bool>,
}

#[napi(object)]
pub struct JsWriteOptions {
    pub enabled: bool,
    pub passes: Option<u32>,
}

#[napi(object)]
pub struct JsRemoveUnusedVarsOptions {
    pub root: Option<String>,
    pub path: Vec<String>,
    pub ignore_patterns: Option<Vec<String>>,
    pub no_unused_vars_config: Option<Either<String, JsNoUnusedVarsOptions>>,
    pub write: Option<Either<bool, JsWriteOptions>>,
    pub include_removals: Option<bool>,
    pub threads: Option<u32>,
}

#[napi(object)]
pub struct JsRemoval {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub kind: String,
}

#[napi(object)]
pub struct JsFileResult {
    /// Absolute file path.
    pub path: String,
    /// Full updated source when `write` is false and the file changed.
    pub updated: Option<String>,
    /// Included only when `includeRemovals` is true.
    pub removals: Option<Vec<JsRemoval>>,
    /// 1-based pass index when `write: { enabled: true, passes }` is used.
    pub pass: Option<u32>,
}

#[napi(object)]
pub struct JsRunError {
    /// Absolute file path, when the error belongs to a file.
    pub path: Option<String>,
    pub message: String,
}

#[napi(object)]
pub struct JsRemoveUnusedVarsResult {
    pub results: Vec<JsFileResult>,
    pub errors: Vec<JsRunError>,
}

#[napi(js_name = "removeUnusedVars")]
pub fn remove_unused_vars_js(
    options: JsRemoveUnusedVarsOptions,
) -> Result<JsRemoveUnusedVarsResult> {
    let root = options
        .root
        .ok_or_else(|| Error::new(Status::InvalidArg, "root is required"))?;
    let threads = options
        .threads
        .map_or_else(default_threads, |value| value as usize);
    let no_unused_vars_config = convert_config(options.no_unused_vars_config)?;
    let write = convert_write(options.write)?;
    let result = remove_unused_vars(RemoveUnusedVarsOptions {
        root: PathBuf::from(root),
        path: options.path,
        ignore_patterns: options.ignore_patterns.unwrap_or_default(),
        no_unused_vars_config,
        write,
        include_removals: options.include_removals.unwrap_or(false),
        threads,
    })
    .map_err(|message| Error::new(Status::InvalidArg, message))?;

    Ok(JsRemoveUnusedVarsResult {
        results: result
            .results
            .into_iter()
            .map(|file| JsFileResult {
                path: file.path.to_string_lossy().into_owned(),
                updated: file.updated,
                removals: file.removals.map(|removals| {
                    removals
                        .into_iter()
                        .map(|removal| JsRemoval {
                            name: removal.name,
                            start: removal.start,
                            end: removal.end,
                            kind: removal.kind,
                        })
                        .collect()
                }),
                pass: file.pass,
            })
            .collect(),
        errors: result
            .errors
            .into_iter()
            .map(|error| JsRunError {
                path: error.path.map(|path| path.to_string_lossy().into_owned()),
                message: error.message,
            })
            .collect(),
    })
}

fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

fn convert_write(write: Option<Either<bool, JsWriteOptions>>) -> Result<WriteOption> {
    match write {
        None => Ok(WriteOption::Disabled),
        Some(Either::A(enabled)) => Ok(WriteOption::from(enabled)),
        Some(Either::B(options)) => {
            if !options.enabled {
                return Ok(WriteOption::Disabled);
            }
            if let Some(passes) = options.passes {
                if passes < 1 {
                    return invalid("passes must be at least 1");
                }
                Ok(WriteOption::Enabled {
                    passes: Some(passes as usize),
                })
            } else {
                Ok(WriteOption::Enabled { passes: None })
            }
        }
    }
}

fn convert_config(
    config: Option<Either<String, JsNoUnusedVarsOptions>>,
) -> Result<NoUnusedVarsConfig> {
    match config {
        None => Ok(NoUnusedVarsConfig::default()),
        Some(Either::A(vars)) => Ok(NoUnusedVarsConfig::Vars(parse_vars(&vars)?)),
        Some(Either::B(config)) => {
            let mut options = NoUnusedVarsOptions::default();
            options.vars_ignore_pattern = IgnorePattern::None;
            options.args_ignore_pattern = IgnorePattern::None;
            if let Some(vars) = config.vars {
                options.vars = parse_vars(&vars)?;
            }
            if let Some(pattern) = config.vars_ignore_pattern {
                options.vars_ignore_pattern = parse_pattern(pattern)?;
            }
            if let Some(args) = config.args {
                options.args = parse_args(&args)?;
            }
            if let Some(pattern) = config.args_ignore_pattern {
                options.args_ignore_pattern = parse_pattern(pattern)?;
            }
            if let Some(value) = config.ignore_rest_siblings {
                options.ignore_rest_siblings = value;
            }
            if let Some(value) = config.caught_errors {
                options.caught_errors = match value.as_str() {
                    "all" => CaughtErrors::all(),
                    "none" => CaughtErrors::none(),
                    _ => return invalid("caughtErrors must be \"all\" or \"none\""),
                };
            }
            if let Some(pattern) = config.caught_errors_ignore_pattern {
                options.caught_errors_ignore_pattern = parse_pattern(pattern)?;
            }
            if let Some(pattern) = config.destructured_array_ignore_pattern {
                options.destructured_array_ignore_pattern = parse_pattern(pattern)?;
            }
            if let Some(value) = config.ignore_class_with_static_init_block {
                options.ignore_class_with_static_init_block = value;
            }
            if let Some(value) = config.ignore_using_declarations {
                options.ignore_using_declarations = value;
            }
            if let Some(value) = config.report_vars_only_used_as_types {
                options.report_vars_only_used_as_types = value;
            }
            Ok(NoUnusedVarsConfig::Options(options))
        }
    }
}

fn parse_vars(value: &str) -> Result<VarsOption> {
    match value {
        "all" => Ok(VarsOption::All),
        "local" => Ok(VarsOption::Local),
        _ => invalid("vars must be \"all\" or \"local\""),
    }
}

fn parse_args(value: &str) -> Result<ArgsOption> {
    match value {
        "after-used" => Ok(ArgsOption::AfterUsed),
        "all" => Ok(ArgsOption::All),
        "none" => Ok(ArgsOption::None),
        _ => invalid("args must be \"after-used\", \"all\", or \"none\""),
    }
}

fn parse_pattern(value: String) -> Result<IgnorePattern<lazy_regex::Regex>> {
    IgnorePattern::try_from(Some(value.as_str())).map_err(|error| {
        Error::new(
            Status::InvalidArg,
            format!("invalid ignore pattern: {error}"),
        )
    })
}

fn invalid<T>(message: &str) -> Result<T> {
    Err(Error::new(Status::InvalidArg, message))
}
