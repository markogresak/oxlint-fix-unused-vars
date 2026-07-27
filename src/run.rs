use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use rayon::prelude::*;

use crate::{
    parse::process_source, paths::expand_paths, FileResult, RemoveUnusedVarsOptions,
    RemoveUnusedVarsResult, RunError, WriteOption,
};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn remove_unused_vars(
    options: RemoveUnusedVarsOptions,
) -> Result<RemoveUnusedVarsResult, String> {
    validate(&options)?;
    let paths = match expand_paths(&options.root, &options.path, &options.ignore_patterns) {
        Ok(paths) => paths,
        Err(message) if message.starts_with("no files found") => {
            return Ok(RemoveUnusedVarsResult {
                results: Vec::new(),
                errors: vec![RunError {
                    path: None,
                    message,
                }],
            });
        }
        Err(message) => return Err(message),
    };
    let detector_options = options.no_unused_vars_config.resolve();
    let threads = options.threads.min(max_threads());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|error| format!("failed to create thread pool: {error}"))?;

    let write = options.write.is_enabled();
    let max_passes = options.write.max_passes().unwrap_or(1);
    let track_pass = matches!(options.write, WriteOption::Enabled { passes: Some(_) });

    let mut results = Vec::new();
    let mut errors = Vec::new();

    for pass in 1..=max_passes {
        let processed = pool.install(|| {
            paths
                .par_iter()
                .map(|path| {
                    process_file(
                        path,
                        &detector_options,
                        write,
                        options.include_removals,
                    )
                })
                .collect::<Vec<_>>()
        });

        let mut pass_results = Vec::new();
        let mut pass_errors = Vec::new();
        let mut wrote_any = false;
        for (path, result) in paths.iter().zip(processed) {
            match result {
                Ok(Some(mut result)) => {
                    if track_pass {
                        result.pass = Some(pass as u32);
                    }
                    if write {
                        wrote_any = true;
                    }
                    pass_results.push(result);
                }
                Ok(None) => {}
                Err(message) => pass_errors.push(RunError {
                    path: Some(path.clone()),
                    message,
                }),
            }
        }
        pass_results.sort_by(|left, right| left.path.cmp(&right.path));
        pass_errors.sort_by(|left, right| left.path.cmp(&right.path));
        // Append without merging across passes — keep this path fast and simple.
        results.extend(pass_results);
        errors.extend(pass_errors);

        if !wrote_any {
            break;
        }
    }

    Ok(RemoveUnusedVarsResult { results, errors })
}

fn validate(options: &RemoveUnusedVarsOptions) -> Result<(), String> {
    if options.root.as_os_str().is_empty() {
        return Err("root is required".to_owned());
    }
    if !options.root.is_absolute() {
        return Err("root must be absolute".to_owned());
    }
    if !options.root.exists() {
        return Err("root must exist".to_owned());
    }
    if !options.root.is_dir() {
        return Err("root must be a directory".to_owned());
    }
    if options.threads < 1 {
        return Err("threads must be at least 1".to_owned());
    }
    if let WriteOption::Enabled {
        passes: Some(passes),
    } = options.write
    {
        if passes < 1 {
            return Err("passes must be at least 1".to_owned());
        }
    }
    Ok(())
}

fn process_file(
    path: &Path,
    options: &crate::NoUnusedVarsOptions,
    write: bool,
    include_removals: bool,
) -> Result<Option<FileResult>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("failed to read file: {error}"))?;
    let Some((updated, removals)) = process_source(path, &source, options)? else {
        return Ok(None);
    };
    let changed = updated != source;
    if !changed {
        return Ok(None);
    }
    if write {
        atomic_write(path, updated.as_bytes())
            .map_err(|error| format!("failed to write file: {error}"))?;
    }
    Ok(Some(FileResult {
        path: path.to_path_buf(),
        updated: (!write).then_some(updated),
        removals: include_removals.then_some(removals),
        pass: None,
    }))
}

fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let target = path.canonicalize()?;
    let permissions = fs::metadata(&target)?.permissions();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source");
    let (temporary, mut file) = loop {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), counter));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.set_permissions(permissions)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn max_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, |available| available.get().saturating_mul(4))
        .min(256)
        .max(1)
}
