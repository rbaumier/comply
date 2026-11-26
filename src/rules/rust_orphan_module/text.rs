//! rust-orphan-module backend: reads the module graph `ImportIndex` already
//! built from every `mod foo;` of the workspace, so a partial scan still judges
//! its files against the whole tree.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::is_conventional_rust_crate_root;
use crate::rules::backend::{CheckCtx, TextCheck};
use std::path::Path;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        let path = index.canonical(ctx.path);
        if is_conventional_rust_crate_root(&path) || index.is_declared_rust_module(&path) {
            return Vec::new();
        }
        let Some(manifest) = ctx.project.nearest_cargo_manifest(&path) else {
            return Vec::new();
        };
        // Only `src/` is a module tree: `tests/` subdirectories also hold
        // trybuild cases and fixtures that no `mod` loads.
        let src = manifest.manifest_dir().join("src");
        if !path.starts_with(&src) || manifest.declares_target_at(&path) {
            return Vec::new();
        }
        // A run scoped to a few files indexes no crate root to declare them.
        let indexes_a_crate_root = index
            .indexed_paths()
            .any(|p| p.starts_with(&src) && is_conventional_rust_crate_root(p));
        if !indexes_a_crate_root {
            return Vec::new();
        }
        let name = module_name(&path);
        if index.has_unresolved_rust_mod(name) || index.has_opaque_rust_mods_in(declaring_dir(&path)) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf().into(),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Orphan module `{name}` — no `mod {name};` declaration \
                 loads this file. Either declare it from the parent module or delete \
                 the file. Orphan files are dead code that grep still finds."
            ),
            severity: Severity::Error,
            span: None,
        }]
    }
}

/// The directory the parent module of `path` resolves its `mod`s in.
pub(crate) fn declaring_dir(path: &Path) -> &Path {
    let dir = path.parent().unwrap_or(path);
    if path.file_name().is_some_and(|n| n == "mod.rs") {
        return dir.parent().unwrap_or(dir);
    }
    dir
}

/// The name a `mod` declaration would give the file: its stem, or its
/// directory's name for a `mod.rs`.
pub(crate) fn module_name(path: &Path) -> &str {
    let named = if path.file_name().is_some_and(|n| n == "mod.rs") {
        path.parent().and_then(Path::file_name)
    } else {
        path.file_stem()
    };
    named.and_then(|n| n.to_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::default_static_file_ctx;

    const CARGO_TOML: &str = "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

    /// Writes a crate (its `Cargo.toml` included) and returns the files, relative
    /// to the crate, that the rule flags.
    fn orphans(files: &[(&str, &str)]) -> Vec<String> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), CARGO_TOML).unwrap();
        let sources: Vec<SourceFile> = files
            .iter()
            .map(|&(rel, src)| {
                let path = dir.path().join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, src).unwrap();
                SourceFile { path, language: Language::Rust }
            })
            .collect();
        let refs: Vec<&SourceFile> = sources.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        let root = dir.path().canonicalize().unwrap();
        let mut flagged = Vec::new();
        for (file, &(rel, src)) in sources.iter().zip(files) {
            let ctx = CheckCtx::for_test_full(&file.path, src, &project, default_static_file_ctx());
            if !Check.check(&ctx).is_empty() {
                let canonical = file.path.canonicalize().unwrap();
                assert_eq!(canonical.strip_prefix(&root).unwrap(), Path::new(rel));
                flagged.push(rel.to_owned());
            }
        }
        flagged
    }

    #[test]
    fn flags_a_file_no_mod_declares() {
        let flagged = orphans(&[("src/lib.rs", "mod a;"), ("src/a.rs", ""), ("src/b.rs", "")]);
        assert_eq!(flagged, ["src/b.rs"]);
    }

    #[test]
    fn follows_mod_rs_and_the_child_dir_of_a_module_file() {
        let flagged = orphans(&[
            ("src/lib.rs", "mod a;\nmod b;"),
            ("src/a/mod.rs", "mod x;"),
            ("src/a/x.rs", ""),
            ("src/b.rs", "pub mod y;"),
            ("src/b/y.rs", ""),
            ("src/b/stray.rs", ""),
        ]);
        assert_eq!(flagged, ["src/b/stray.rs"]);
    }

    #[test]
    fn follows_a_path_attribute() {
        let flagged =
            orphans(&[("src/lib.rs", "#[path = \"gen/out.rs\"]\nmod generated;"), ("src/gen/out.rs", "")]);
        assert!(flagged.is_empty());
    }

    #[test]
    fn a_bin_target_resolves_its_modules_beside_its_main() {
        let flagged = orphans(&[
            ("src/bin/tool/main.rs", "mod helper;"),
            ("src/bin/tool/helper.rs", ""),
            ("src/bin/tool/stray.rs", ""),
            ("src/bin/single.rs", ""),
        ]);
        assert_eq!(flagged, ["src/bin/tool/stray.rs"]);
    }

    #[test]
    fn an_integration_test_resolves_its_modules_beside_it_and_tests_are_never_flagged() {
        let flagged = orphans(&[
            ("tests/it.rs", "mod common;"),
            ("tests/common/mod.rs", ""),
            ("tests/ui/compile_fail.rs", ""),
        ]);
        assert!(flagged.is_empty());
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), CARGO_TOML).unwrap();
        std::fs::create_dir_all(dir.path().join("tests/common")).unwrap();
        std::fs::write(dir.path().join("tests/it.rs"), "mod common;").unwrap();
        std::fs::write(dir.path().join("tests/common/mod.rs"), "").unwrap();
        let files = ["tests/it.rs", "tests/common/mod.rs"]
            .map(|rel| SourceFile { path: dir.path().join(rel), language: Language::Rust });
        let refs: Vec<&SourceFile> = files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        assert!(project.import_index().is_declared_rust_module(&dir.path().join("tests/common/mod.rs")));
    }

    #[test]
    fn judges_nothing_when_no_crate_root_is_indexed() {
        assert!(orphans(&[("src/a.rs", "")]).is_empty());
    }

    #[test]
    fn leaves_modules_an_include_or_a_macro_may_declare_alone() {
        let included = orphans(&[
            ("src/lib.rs", "include!(concat!(env!(\"OUT_DIR\"), \"/mods.rs\"));"),
            ("src/a.rs", ""),
            ("src/b/mod.rs", ""),
        ]);
        assert!(included.is_empty());
        let in_macro = orphans(&[("src/lib.rs", "cfg_if::cfg_if! {\n    if #[cfg(unix)] { mod sys; }\n}"), ("src/sys.rs", "")]);
        assert!(in_macro.is_empty());
        let unrelated_macro = orphans(&[("src/lib.rs", "thread_local! { static X: u8 = 0; }"), ("src/a.rs", "")]);
        assert_eq!(unrelated_macro, ["src/a.rs"]);
    }

    #[test]
    fn leaves_a_file_named_by_an_unresolved_mod_alone() {
        let flagged = orphans(&[("src/lib.rs", "mod outer {\n    mod inner;\n}"), ("src/outer/inner.rs", "")]);
        assert!(flagged.is_empty());
    }
}
