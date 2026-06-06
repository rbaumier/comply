//! Shared path classifiers used by multiple rules.
//!
//! Centralised so `unused-file` and `dead-export` agree on what counts as a
//! config file and don't drift apart over time.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::project::ProjectCtx;

thread_local! {
    /// Per-thread memo of `canonicalize`. The project root and each file's
    /// parent directory are canonicalized once per classifier call; the same
    /// directories recur across thousands of files in a run, and the project
    /// root is constant. canonicalize hits the filesystem (one syscall per
    /// path segment), so memoizing collapses the bulk of those syscalls.
    /// Results are deterministic for the duration of a run, so the memo is
    /// output-identical to calling `canonicalize` directly.
    static CANON_CACHE: RefCell<FxHashMap<PathBuf, PathBuf>> =
        RefCell::new(FxHashMap::default());
}

/// `std::fs::canonicalize(p)` with a per-thread memo, falling back to `p`
/// itself on error (same as the previous inline `unwrap_or_else`).
fn canonicalize_cached(p: &Path) -> PathBuf {
    CANON_CACHE.with(|c| {
        if let Some(v) = c.borrow().get(p) {
            return v.clone();
        }
        let v = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        c.borrow_mut().insert(p.to_path_buf(), v.clone());
        v
    })
}

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

/// True when `path` matches a framework entry point via FILES, SUFFIXES, or
/// ROOT_FILES only — does NOT check dirs. Used by `dead-export` to bail out
/// for framework-specific files even when the user has configured additional
/// entrypoints (which disables the dirs bail-out).
pub fn is_framework_specific_entry_point(path: &Path, project: &ProjectCtx) -> bool {
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
    let canon_parent = canonicalize_cached(parent);
    let canon_root = canonicalize_cached(root);
    if canon_parent != canon_root {
        return false;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    project.framework_root_files().any(|entry| entry == stem)
}

/// True when `path` lives under a framework entry_points.dirs directory.
/// Used by `dead-export` to suppress the dirs bail-out only when the user
/// has NOT configured additional entrypoints (backward-compat mode).
pub fn is_in_framework_entry_dir(path: &Path, project: &ProjectCtx) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    project
        .framework_entry_dirs()
        .any(|dir| path_str.contains(dir))
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
    let canon_parent = canonicalize_cached(parent);
    let canon_root = canonicalize_cached(root);
    if canon_parent != canon_root {
        return false;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    project.framework_root_files().any(|entry| entry == stem)
}
