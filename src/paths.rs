use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use globset::GlobBuilder;
use ignore::{gitignore::GitignoreBuilder, WalkBuilder};

pub(crate) fn expand_paths(
    root: &Path,
    patterns: &[String],
    ignore_patterns: &[String],
) -> Result<Vec<PathBuf>, String> {
    let mut ignore_builder = GitignoreBuilder::new(root);
    for pattern in ignore_patterns {
        ignore_builder
            .add_line(None, pattern)
            .map_err(|error| format!("invalid ignorePatterns entry {pattern:?}: {error}"))?;
    }
    let ignores = ignore_builder
        .build()
        .map_err(|error| format!("invalid ignorePatterns: {error}"))?;

    let mut paths = BTreeSet::new();
    for pattern in patterns {
        let absolute = if Path::new(pattern).is_absolute() {
            PathBuf::from(pattern)
        } else {
            root.join(pattern)
        };

        if !has_glob_meta(pattern) {
            if absolute.is_file() {
                if is_supported_extension(&absolute) {
                    add_if_included(root, &absolute, &ignores, &mut paths);
                }
            } else if absolute.is_dir() {
                walk_matching(root, &absolute, None, &ignores, &mut paths)?;
            }
            continue;
        }

        let glob = GlobBuilder::new(&absolute.to_string_lossy())
            .literal_separator(true)
            .build()
            .map_err(|error| format!("invalid path glob {pattern:?}: {error}"))?
            .compile_matcher();
        let base = glob_base(&absolute);
        if base.exists() {
            walk_matching(root, &base, Some(&glob), &ignores, &mut paths)?;
        }
    }

    if paths.is_empty() {
        return Err(format!(
            "no files found under root {} after expanding path and applying ignorePatterns",
            root.display()
        ));
    }
    Ok(paths.into_iter().collect())
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern
        .bytes()
        .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b'{'))
}

fn glob_base(path: &Path) -> PathBuf {
    let mut base = PathBuf::new();
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        if has_glob_meta(&text) {
            break;
        }
        base.push(component);
    }
    if base.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        base
    }
}

fn walk_matching(
    root: &Path,
    base: &Path,
    glob: Option<&globset::GlobMatcher>,
    ignores: &ignore::gitignore::Gitignore,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let mut builder = WalkBuilder::new(base);
    builder
        .hidden(false)
        .ignore(false)
        .parents(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false);

    for entry in builder.build() {
        let entry = entry.map_err(|error| format!("failed to walk {}: {error}", base.display()))?;
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && is_supported_extension(path)
            && glob.is_none_or(|matcher| matcher.is_match(path))
        {
            add_if_included(root, path, ignores, paths);
        }
    }
    Ok(())
}

fn add_if_included(
    root: &Path,
    path: &Path,
    ignores: &ignore::gitignore::Gitignore,
    paths: &mut BTreeSet<PathBuf>,
) {
    let (Ok(canonical_root), Ok(canonical_path)) = (root.canonicalize(), path.canonicalize())
    else {
        return;
    };
    let Ok(relative) = canonical_path.strip_prefix(canonical_root) else {
        return;
    };
    if !ignores
        .matched_path_or_any_parents(relative, false)
        .is_ignore()
    {
        paths.insert(path.to_path_buf());
    }
}

fn is_supported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts")
    )
}

#[cfg(test)]
mod tests {
    use super::{has_glob_meta, is_supported_extension};
    use std::path::Path;

    #[test]
    fn recognizes_brace_globs() {
        assert!(has_glob_meta("**/*.{js,ts}"));
    }

    #[test]
    fn limits_walked_files_to_supported_extensions() {
        assert!(is_supported_extension(Path::new("source.mts")));
        assert!(!is_supported_extension(Path::new("component.vue")));
    }
}
