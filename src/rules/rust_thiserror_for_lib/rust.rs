//! rust-thiserror-for-lib backend.
//!
//! Skips `main.rs` / `src/bin/` (application crates), any file that
//! already mentions `thiserror`, `no_std` crates, and non-`pub` types.
//! In what remains, flags `pub enum_item` declarations whose name contains
//! `Error` — the signal that this is a library-facing error type which
//! should derive `thiserror::Error` rather than hand-roll `Display`/`Error`.

use crate::diagnostic::{Diagnostic, Severity};
use std::path::{Path, PathBuf};

crate::ast_check! { on ["enum_item"] => |node, source, ctx, diagnostics|
    let path_str = ctx.path.to_string_lossy();
    if path_str.contains("main.rs") || path_str.contains("src/bin/") { return; }
    if ctx.source_contains("thiserror") { return; }
    if is_no_std_crate(ctx.path) { return; }

    if !is_pub(node, source) { return; }

    let Some(name) = node.child_by_field_name("name") else { return; };
    let Ok(name_text) = name.utf8_text(source) else { return; };
    if !name_text.contains("Error") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `#[derive(thiserror::Error)]` for library error types — avoids boilerplate `Display` impls.".into(),
        Severity::Warning,
    ));
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
        {
            return text == "pub";
        }
    }
    false
}

fn nearest_manifest(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        let cargo_toml = d.join("Cargo.toml");
        if cargo_toml.is_file() {
            return Some(cargo_toml);
        }
        dir = d.parent();
    }
    None
}

fn is_no_std_crate(path: &Path) -> bool {
    let Some(manifest) = nearest_manifest(path) else { return false; };
    let Ok(content) = std::fs::read_to_string(&manifest) else { return false; };

    // Signal 1: categories = ["no-std"] in Cargo.toml (published crates like jiff)
    if let Ok(value) = content.parse::<toml::Value>() {
        let has_no_std_category = value
            .get("package")
            .and_then(|p| p.get("categories"))
            .and_then(|c| c.as_array())
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("no-std")));
        if has_no_std_category {
            return true;
        }
    }

    // Signal 2: #![no_std] in src/lib.rs (unpublished / embedded crates)
    if let Some(root) = manifest.parent() {
        if let Ok(lib_src) = std::fs::read_to_string(root.join("src/lib.rs")) {
            if lib_src.contains("#![no_std]") {
                return true;
            }
        }
    }

    false
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;
    use std::fs;
    use tempfile::TempDir;

    fn run(s: &str) -> Vec<Diagnostic> {
        // Absolute path with no Cargo.toml ancestor so no_std detection
        // doesn't pick up the comply project's own Cargo.toml.
        crate::rules::test_helpers::run_rule(&Check, s, "/nonexistent_cargo_project/src/error.rs")
    }

    #[test]
    fn flags_pub_enum_error_without_thiserror() {
        assert_eq!(run("pub enum MyError { NotFound, Unauthorized }").len(), 1);
    }

    #[test]
    fn allows_enum_with_thiserror() {
        assert!(
            run(
                "#[derive(thiserror::Error)]\npub enum MyError { #[error(\"not found\")] NotFound }"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_main_rs() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "pub enum MyError { Fail }", "src/main.rs");
        assert!(diags.is_empty());
    }

    #[test]
    fn does_not_flag_pub_crate_enum() {
        assert!(run("pub(crate) enum MyError { Fail }").is_empty());
    }

    #[test]
    fn does_not_flag_no_std_crate_via_categories() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"jiff\"\ncategories = [\"no-std\"]\n",
        )
        .unwrap();
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "pub enum MyError { Fail }",
            dir.path().join("src/error.rs"),
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn does_not_flag_no_std_crate_via_lib_rs_attribute() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"nostd\"\n").unwrap();
        fs::write(dir.path().join("src/lib.rs"), "#![no_std]\n").unwrap();
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "pub enum MyError { Fail }",
            dir.path().join("src/error.rs"),
        );
        assert!(diags.is_empty());
    }
}
