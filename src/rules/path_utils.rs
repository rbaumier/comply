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

/// True when `path` lives inside a jscodeshift codemod fixture directory: an
/// ancestor directory whose name ends in `.test` (e.g.
/// `menu-item-primary-text.test/actual.js`). These directories hold the
/// pre-/post-transformation snippets a codemod operates on; their JSX
/// references components without importing them on purpose, so identifier-
/// resolution rules must not lint them.
pub fn is_codemod_fixture_file(path: &Path) -> bool {
    path.parent().is_some_and(|parent| {
        parent.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|seg| seg.ends_with(".test"))
        })
    })
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

/// True when `file_name` is a SvelteKit route file: a `+`-prefixed basename
/// from the framework's file-system routing set (`+page`, `+layout`,
/// `+server`, `+error` with `.svelte`/`.ts`/`.js` and an optional `.server`
/// segment). These are discovered by the router at build time, never imported.
pub fn is_sveltekit_route_file(file_name: &str) -> bool {
    let Some(rest) = file_name.strip_prefix('+') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    matches!(
        parts.as_slice(),
        ["page" | "layout" | "error", "svelte"]
            | ["page" | "layout", "js" | "ts"]
            | ["page" | "layout", "server", "js" | "ts"]
            | ["server", "js" | "ts"]
    )
}

/// True when `path` is a SvelteKit route file (`+page.svelte`,
/// `+page.server.ts`, `+server.ts`, …) located under a `routes/` directory in
/// a project where SvelteKit is detected. SvelteKit's file-system router
/// consumes these by path, so nothing imports them — they are implicit entry
/// points. The `routes/` ancestor and detection gate keep the exemption from
/// covering an unrelated `+`-named file.
fn is_sveltekit_route_entry(path: &Path, project: &ProjectCtx) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !is_sveltekit_route_file(name) {
        return false;
    }
    if !path
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
    {
        return false;
    }
    project.has_framework("svelte")
        || project.frameworks_for_path(path).iter().any(|f| f.name == "svelte")
}

/// True when `path` matches an entry point declared by any detected
/// framework. This covers file-based routers, generated route trees, and
/// framework-owned files whose exports/import reachability is implicit.
pub fn is_framework_entry_point(path: &Path, project: &ProjectCtx) -> bool {
    if is_sveltekit_route_entry(path, project) {
        return true;
    }

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

    // Fall back to the framework owning this file via its nearest package.json:
    // a framework app nested in a subdirectory (a Next.js example under a
    // library's `app/`, a monorepo package) is invisible to the root-anchored
    // `detected_frameworks`. Its `dirs`/`files`/`suffixes` are path-relative,
    // so they identify file-system-routed entry points (Next.js `pages/`,
    // Remix `routes/`, SvelteKit `src/routes/`) regardless of detection depth.
    for fw in project.frameworks_for_path(path) {
        if fw.entry_points.dirs.iter().any(|dir| path_str.contains(dir.as_str())) {
            return true;
        }
        if fw.entry_points.files.iter().any(|entry| entry == name) {
            return true;
        }
        if fw
            .entry_points
            .file_suffixes
            .iter()
            .any(|suffix| name.ends_with(suffix.as_str()))
        {
            return true;
        }
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
