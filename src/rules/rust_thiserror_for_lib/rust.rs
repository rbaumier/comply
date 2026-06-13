//! rust-thiserror-for-lib backend.
//!
//! Skips `main.rs` / `src/bin/` (application crates) and any file that
//! already mentions `thiserror`. In what remains, flags `enum_item`
//! declarations that are truly `pub` (bare `pub` only — `pub(crate)`,
//! `pub(super)`, and `pub(in …)` are crate-internal, not library API)
//! and whose name contains `Error` — the signal that this is a
//! library-facing error type which should derive `thiserror::Error`
//! rather than hand-roll `Display`/`Error`.
//!
//! ## no_std exemption
//!
//! `thiserror` generates `impl std::error::Error`, which requires `std`.
//! In `no_std` crates a manual `core::fmt::Display` impl is the correct
//! pattern, so the rule is silenced when the file source mentions
//! `no_std` (covering `#![no_std]` and `#![cfg_attr(not(feature =
//! "std"), no_std)]`) or when the nearest `Cargo.toml` lists `"no-std"`
//! in `[package].categories`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["enum_item"] => |node, source, ctx, diagnostics|
    let path_str = ctx.path.to_string_lossy();
    if path_str.contains("main.rs") || path_str.contains("src/bin/") { return; }
    if ctx.source_contains("thiserror") { return; }
    if ctx.source_contains("no_std") { return; }

    if !is_pub(node, source) { return; }

    let Some(name) = node.child_by_field_name("name") else { return; };
    let Ok(name_text) = name.utf8_text(source) else { return; };
    if !name_text.contains("Error") { return; }

    if ctx.project.nearest_cargo_manifest(ctx.path).is_some_and(|m| m.is_no_std()) { return; }

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
            && text.trim() == "pub"
        {
            return true;
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
        crate::rules::test_helpers::run_rule(&Check, s, "src/error.rs")
    }

    /// Run on a file in `dir/src/x.rs` with the given `Cargo.toml` contents,
    /// so the `nearest_cargo_manifest` no-std check resolves the temp crate's
    /// manifest.
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join("src/x.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
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

    // ── no_std / visibility regression tests (Closes #999) ──────────────

    const NO_STD_CARGO_TOML: &str = r#"
[package]
name = "jiff-like"
version = "0.1.0"
edition = "2021"
categories = ["no-std"]
"#;

    const STD_CARGO_TOML: &str = r#"
[package]
name = "std-lib"
version = "0.1.0"
edition = "2021"
"#;

    /// Regression for #999: `pub(crate)` error enums are crate-internal,
    /// not library API — must not be flagged (jiff `src/util/b.rs:709`).
    #[test]
    fn allows_pub_crate_enum_error() {
        let src = "pub(crate) enum SpecialBoundsError { Lower, Upper }\n\
                   impl core::fmt::Display for SpecialBoundsError {\n\
                       fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { write!(f, \"bounds\") }\n\
                   }";
        assert!(
            run(src).is_empty(),
            "must not flag pub(crate) error enums — they are not public API"
        );
    }

    /// Regression for #999: a crate declaring `categories = ["no-std"]`
    /// cannot use `thiserror` (it generates `impl std::error::Error`).
    #[test]
    fn allows_pub_enum_error_in_no_std_crate() {
        assert!(
            run_on_with_cargo(NO_STD_CARGO_TOML, "pub enum MyError { Fail }").is_empty(),
            "must not flag pub error enums in a no-std crate"
        );
    }

    /// Regression for #999: a file whose source declares `#![no_std]`
    /// must not be flagged, even without a manifest hint.
    #[test]
    fn allows_pub_enum_error_in_no_std_source() {
        assert!(
            run("#![no_std]\npub enum MyError { Fail }").is_empty(),
            "must not flag pub error enums in a #![no_std] file"
        );
    }

    #[test]
    fn still_flags_pub_enum_error_in_std_crate() {
        assert_eq!(
            run_on_with_cargo(STD_CARGO_TOML, "pub enum MyError { Fail }").len(),
            1,
            "must keep flagging pub error enums in std crates"
        );
    }
}
