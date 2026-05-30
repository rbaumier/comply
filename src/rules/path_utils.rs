//! Shared path classifiers used by multiple rules.
//!
//! Centralised so `unused-file` and `dead-export` agree on what counts as a
//! config file and don't drift apart over time.

use std::path::Path;

use crate::project::ProjectCtx;

/// Returns true when the resolved target of a relative import specifier would
/// be matched by a `.gitignore` pattern found anywhere up the directory tree
/// from `base_dir`. Suppresses false positives on auto-generated files
/// (e.g. TanStack Router's `routeTree.gen.ts`) that are intentionally absent
/// from source control but exist at dev/build time.
pub fn is_relative_specifier_gitignored(base_dir: &Path, specifier: &str) -> bool {
    use ignore::gitignore::Gitignore;

    if !specifier.starts_with('.') {
        return false;
    }

    // Normalize out `.` and `..` components so `base_dir.join("./foo")` becomes
    // `base_dir/foo` — required for `ignore::Gitignore::matched` to strip the
    // root prefix correctly when comparing against gitignore patterns.
    let raw = base_dir.join(specifier);
    let mut components = Vec::new();
    for c in raw.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            c => components.push(c),
        }
    }
    let resolved: std::path::PathBuf = components.into_iter().collect();

    // Append extensions with string concatenation, not `with_extension`, because
    // `with_extension` replaces the existing extension (e.g. `foo.gen` + `.ts` →
    // `foo.ts`) while we need to append (→ `foo.gen.ts`).
    let base = resolved.to_string_lossy().into_owned();
    let candidates = [
        resolved.clone(),
        std::path::PathBuf::from(format!("{base}.ts")),
        std::path::PathBuf::from(format!("{base}.tsx")),
        std::path::PathBuf::from(format!("{base}.js")),
        std::path::PathBuf::from(format!("{base}.jsx")),
    ];

    let mut dir: Option<&Path> = Some(base_dir);
    while let Some(d) = dir {
        let gitignore_path = d.join(".gitignore");
        if gitignore_path.exists() {
            let (gi, _) = Gitignore::new(&gitignore_path);
            if candidates.iter().any(|c| gi.matched(c, false).is_ignore()) {
                return true;
            }
        }
        if d.join(".git").exists() {
            break;
        }
        dir = d.parent();
    }

    false
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
