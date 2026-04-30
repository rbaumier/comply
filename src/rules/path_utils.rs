//! Shared path classifiers used by multiple rules.
//!
//! Centralised so `unused-file` and `dead-export` agree on what counts as a
//! config file and don't drift apart over time.

use std::path::Path;

use crate::project::ProjectCtx;

/// True if `path` is a build/tooling config file. Matches `*.config.*`
/// (e.g. `vite.config.ts`, `jest.config.js`) and dotfile-rc entries
/// (e.g. `.eslintrc.js`, `.babelrc.ts`).
pub fn is_config_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem.ends_with(".config") {
        return true;
    }
    if name.starts_with('.') && stem.ends_with("rc") {
        return true;
    }
    false
}

/// True when `path` matches an entry point declared by any detected
/// framework. This covers file-based routers, generated route trees, and
/// framework-owned files whose exports/import reachability is implicit.
pub fn is_framework_entry_point(path: &Path, project: &ProjectCtx) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if project
        .framework_entry_dirs()
        .any(|dir| path_str.contains(dir))
    {
        return true;
    }

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if project.framework_entry_files().any(|entry| entry == name) {
        return true;
    }
    if project
        .framework_entry_file_suffixes()
        .any(|suffix| name.ends_with(suffix))
    {
        return true;
    }

    let Some(root) = project.project_root.as_deref() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if canon_parent != canon_root {
        return false;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    project.framework_root_files().any(|entry| entry == stem)
}
