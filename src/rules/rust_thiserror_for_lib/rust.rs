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
//! Names ending in `Kind` are exempt: by Rust convention (cf.
//! `std::io::ErrorKind`) a `*ErrorKind` / `*Kind` enum is an error
//! *classifier*, not an error type — it does not implement
//! `std::error::Error`, so deriving `thiserror::Error` does not apply.
//!
//! ## no_std exemption
//!
//! `thiserror` generates `impl std::error::Error`, which requires `std`.
//! In `no_std` crates a manual `core::fmt::Display` impl is the correct
//! pattern, so the rule is silenced when the file source mentions
//! `no_std` (covering `#![no_std]` and `#![cfg_attr(not(feature =
//! "std"), no_std)]`), when the crate root (`src/lib.rs` / `src/main.rs`)
//! declares `#![no_std]` — which exempts every file in the crate, not just
//! the one declaring it — or when the nearest `Cargo.toml` lists `"no-std"`
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
    // `*ErrorKind` / `*Kind` enums are error CLASSIFIERS (cf. `std::io::ErrorKind`),
    // not error types — they don't implement `std::error::Error`, so deriving
    // `thiserror::Error` would be pointless.
    if name_text.ends_with("Kind") { return; }

    if ctx.project.nearest_cargo_manifest(ctx.path).is_some_and(|m| m.is_no_std()) { return; }
    if ctx.project.crate_root_is_no_std(ctx.path) { return; }

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
    fn flags_pub_app_error_without_thiserror() {
        assert_eq!(run("pub enum AppError { Network }").len(), 1);
    }

    /// Regression for #4402: `*ErrorKind` enums are error classifiers
    /// (cf. `std::io::ErrorKind`), not error types — they don't implement
    /// `std::error::Error`, so `thiserror::Error` does not apply.
    #[test]
    fn allows_pub_error_kind_classifier() {
        assert!(
            run("pub enum CommitProcessingErrorKind { Io, Parse, Json, Other }").is_empty(),
            "must not flag *ErrorKind classifier enums"
        );
    }

    #[test]
    fn allows_pub_plain_error_kind() {
        assert!(
            run("pub enum ErrorKind { NotFound, Other }").is_empty(),
            "must not flag *Kind classifier enums"
        );
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

    /// Run the rule on a submodule file `dir/src/util/error.rs` in a crate
    /// whose root is `dir/src/lib.rs`, so `crate_root_is_no_std` resolves the
    /// crate's `#![no_std]` declaration from a *different* file than the one
    /// being flagged.
    fn run_on_submodule_with_lib(
        cargo_toml_contents: &str,
        lib_rs: &str,
        submodule_src: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src/util")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), lib_rs).unwrap();
        let submodule_path = dir.path().join("src/util/error.rs");
        fs::write(&submodule_path, submodule_src).unwrap();
        crate::rules::test_helpers::run_rule(&Check, submodule_src, &submodule_path)
    }

    const MATCH_ERROR_KIND_SRC: &str = "pub enum MatchErrorKind { InvalidInputAnchored, UnsupportedEmpty }\n\
         impl core::fmt::Display for MatchErrorKind {\n\
             fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { Ok(()) }\n\
         }";

    /// Regression for #4475 (aho-corasick): the crate root `src/lib.rs`
    /// declares `#![no_std]` while the flagged enum lives in the submodule
    /// `src/util/error.rs`. The categories list has no `"no-std"` entry, so
    /// only the crate-root `#![no_std]` can exempt it. `MatchErrorKind` is
    /// renamed here to drop the `*Kind` suffix to prove the exemption, not the
    /// classifier rule, is what silences the diagnostic.
    #[test]
    fn allows_submodule_enum_when_crate_root_is_no_std() {
        let src = MATCH_ERROR_KIND_SRC.replace("MatchErrorKind", "MatchError");
        assert!(
            run_on_submodule_with_lib(STD_CARGO_TOML, "#![no_std]\n", &src).is_empty(),
            "must not flag a submodule enum when the crate root declares #![no_std]"
        );
    }

    /// Negative counterpart of #4475: same submodule enum, but the crate root
    /// is a plain `std` crate. The diagnostic must still fire — the crate-root
    /// exemption only triggers on a real `#![no_std]` declaration.
    #[test]
    fn still_flags_submodule_enum_in_std_crate() {
        let src = MATCH_ERROR_KIND_SRC.replace("MatchErrorKind", "MatchError");
        assert_eq!(
            run_on_submodule_with_lib(STD_CARGO_TOML, "", &src).len(),
            1,
            "must keep flagging a submodule enum when the crate root is std"
        );
    }
}
