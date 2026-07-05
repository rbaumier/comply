//! rust-thiserror-for-lib backend.
//!
//! Skips `main.rs` / `src/bin/` and any file in a binary-only crate (no
//! `[lib]` table and no `src/lib.rs` — an application crate whose error types
//! have no downstream library consumers), plus any file that already uses a
//! derive-based error-handling library. In what remains,
//! flags `enum_item` declarations that are truly `pub` (bare `pub` only —
//! `pub(crate)`, `pub(super)`, and `pub(in …)` are crate-internal, not
//! library API), whose name contains `Error`, and for which the file
//! hand-rolls the boilerplate the derive would replace — an
//! `impl Display for <Enum>` or `impl …Error for <Enum>` block. The name
//! signals a library-facing error type; the impl is the `Display`/`Error`
//! boilerplate `#[derive(thiserror::Error)]` removes. An enum with no such
//! impl — an internal result-discriminator returned by a `get()` /
//! `bare_name()` and pattern-matched by the caller — has nothing for the
//! derive to replace (deriving it would only *add* an `#[error("…")]`
//! message per variant), so it is not flagged. This also exempts `*Kind`
//! error *classifiers* (cf. `std::io::ErrorKind`), which never hand-roll
//! `std::error::Error`.
//!
//! ## error-derive-library exemption
//!
//! The rule's intent is "derive library error types from a structured error
//! library", not "use `thiserror` specifically". A crate using any recognized
//! error-derive library (`thiserror`, `snafu`, `miette`, `derive_more`,
//! `error-stack`) already satisfies that intent, so it is exempt — detected
//! either from the file source importing the library (`use snafu::…`,
//! `#[derive(Snafu)]`) or from the nearest `Cargo.toml` declaring it as a
//! dependency (covering a derive that lives in a sibling file).
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
use crate::rules::rust_helpers::trait_base_name;

/// Crate import roots of derive-based error-handling libraries. A file that
/// imports one of these (`use snafu::…`, `use miette::…`) or names one in a
/// derive (`#[derive(Snafu)]`) already derives its error types from a structured
/// library, so the rule must not push it toward `thiserror`. Keyed on the Rust
/// path segment (`derive_more`, `error_stack` — never the `-` crate spelling),
/// mirroring the manifest-side crate set recognized by `CargoManifest`.
const ERROR_DERIVE_IMPORT_ROOTS: &[&str] =
    &["thiserror", "snafu", "miette", "derive_more", "error_stack"];

crate::ast_check! { on ["enum_item"] => |node, source, ctx, diagnostics|
    let path_str = ctx.path.to_string_lossy();
    if path_str.contains("main.rs") || path_str.contains("src/bin/") { return; }
    // A file using any error-derive library already satisfies the rule's intent;
    // only the absence of every such library is a true hand-rolled error type.
    if ERROR_DERIVE_IMPORT_ROOTS.iter().any(|root| ctx.source_contains(root)) { return; }
    if ctx.source_contains("no_std") { return; }

    if !is_pub(node, source) { return; }

    let Some(name) = node.child_by_field_name("name") else { return; };
    let Ok(name_text) = name.utf8_text(source) else { return; };
    if !name_text.contains("Error") { return; }
    // Precondition: only fire when the boilerplate the derive would replace
    // actually exists (see the module docstring) — otherwise there is nothing
    // to derive away.
    if !has_hand_rolled_display_or_error_impl(node, source, name_text) { return; }

    // A binary-only crate (no `[lib]` table, no `src/lib.rs`) builds no library
    // target, so its error types have no downstream consumers — they are
    // application-internal, like `main.rs`/`src/bin/` code, and need no derive.
    if ctx.project.nearest_cargo_manifest(ctx.path).is_some_and(|m| m.is_binary_only()) { return; }
    // The error-derive library may be a crate dependency whose derive lives in a
    // sibling file; the manifest carries it crate-wide where this file's source
    // alone cannot.
    if ctx.project.nearest_cargo_manifest(ctx.path).is_some_and(|m| m.uses_error_derive_crate()) { return; }
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

/// True when the file hand-rolls an `impl Display for <enum_name>` or
/// `impl …Error for <enum_name>` block — the `Display`/`Error` boilerplate that
/// `#[derive(thiserror::Error)]` replaces. The trait is matched by its last
/// path segment, so `Display` / `fmt::Display` / `std::fmt::Display` and
/// `Error` / `error::Error` / `std::error::Error` all count; the Self type is
/// matched by its base name, so `<enum_name>` and `<enum_name><'a>` both count.
fn has_hand_rolled_display_or_error_impl(
    node: tree_sitter::Node,
    source: &[u8],
    enum_name: &str,
) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "impl_item"
            && let Some(target) = current.child_by_field_name("type")
            && trait_base_name(target, source) == Some(enum_name)
            && let Some(trait_node) = current.child_by_field_name("trait")
            && trait_base_name(trait_node, source).is_some_and(|t| t == "Display" || t == "Error")
        {
            return true;
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
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
    /// so the `nearest_cargo_manifest` checks resolve the temp crate's manifest.
    /// An empty `src/lib.rs` is created so the crate is a genuine library target
    /// (`is_binary_only` is false) — the rule only flags library error types.
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        let src_path = dir.path().join("src/x.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_pub_enum_error_without_thiserror() {
        // Isolated crate manifest: the relative-path `run` helper would resolve
        // to comply's own `Cargo.toml`, which depends on an error-derive library
        // (`miette`) and would exempt the whole crate. The enum hand-rolls the
        // `Display` boilerplate the derive would replace, so it is flagged.
        let src = "pub enum MyError { NotFound, Unauthorized }\n\
                   impl std::fmt::Display for MyError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"err\") }\n\
                   }";
        assert_eq!(run_on_with_cargo(STD_CARGO_TOML, src).len(), 1);
    }

    #[test]
    fn flags_pub_app_error_without_thiserror() {
        let src = "pub enum AppError { Network }\n\
                   impl std::fmt::Display for AppError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"network\") }\n\
                   }";
        assert_eq!(run_on_with_cargo(STD_CARGO_TOML, src).len(), 1);
    }

    /// Regression for #7295: a `pub *Error` enum with no hand-rolled
    /// `Display`/`Error` impl (an internal result discriminator like typst's
    /// `IntLiteralError`, returned by `get()` and matched by the caller) has no
    /// boilerplate for the derive to replace, so it must not be flagged. Runs in
    /// an isolated std crate so only the impl-presence precondition — not the
    /// manifest exemption — can silence it.
    #[test]
    fn allows_pub_error_enum_without_hand_rolled_impl() {
        let src = "pub enum IntLiteralError<'a> { PosOverflow, InvalidDigit(&'a str) }";
        assert!(
            run_on_with_cargo(STD_CARGO_TOML, src).is_empty(),
            "must not flag a *Error enum with no hand-rolled Display/Error impl"
        );
    }

    /// Counterpart of #7295: a `pub *Error` enum that *does* hand-roll `Display`
    /// + `impl std::error::Error` (typst's `path.rs` error types) still has the
    /// boilerplate the derive replaces, so it stays flagged.
    #[test]
    fn still_flags_pub_error_enum_with_hand_rolled_display_and_error() {
        let src = "pub enum PathError { NotFound }\n\
                   impl std::fmt::Display for PathError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"not found\") }\n\
                   }\n\
                   impl std::error::Error for PathError {}";
        assert_eq!(run_on_with_cargo(STD_CARGO_TOML, src).len(), 1);
    }

    /// A *generic* `pub enum SomeError<'a>` that hand-rolls `Display` for
    /// `SomeError<'a>` still flags: the precondition strips the `<'a>` from the
    /// impl's Self type when matching it against the enum name, so the generic
    /// positive path is locked (the mirror of the generic #7295 FP).
    #[test]
    fn still_flags_generic_pub_error_enum_with_hand_rolled_display() {
        let src = "pub enum SomeError<'a> { Bad(&'a str) }\n\
                   impl<'a> std::fmt::Display for SomeError<'a> {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"bad\") }\n\
                   }";
        assert_eq!(run_on_with_cargo(STD_CARGO_TOML, src).len(), 1);
    }

    /// Regression for #4402: `*ErrorKind` enums are error classifiers
    /// (cf. `std::io::ErrorKind`), not error types — they don't hand-roll
    /// `std::error::Error`, so `thiserror::Error` does not apply. Now silenced
    /// by the impl-presence precondition (#7295), which subsumes the former
    /// `*Kind` name-suffix carve-out. Runs in an isolated std crate so the
    /// precondition, not the manifest exemption, is what silences it.
    #[test]
    fn allows_pub_error_kind_classifier() {
        assert!(
            run_on_with_cargo(STD_CARGO_TOML, "pub enum CommitProcessingErrorKind { Io, Parse, Json, Other }")
                .is_empty(),
            "must not flag *ErrorKind classifier enums (no hand-rolled Display/Error impl)"
        );
    }

    #[test]
    fn allows_pub_plain_error_kind() {
        assert!(
            run_on_with_cargo(STD_CARGO_TOML, "pub enum ErrorKind { NotFound, Other }").is_empty(),
            "must not flag *Kind classifier enums (no hand-rolled Display/Error impl)"
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

    /// Regression for #5288 (shepmaster/snafu): `snafu` is itself a derive-based
    /// error library — `#[derive(Snafu)]` provides the same structured error
    /// handling as `#[derive(thiserror::Error)]`, so a file importing it must
    /// not be pushed toward `thiserror`.
    #[test]
    fn allows_enum_with_snafu() {
        assert!(
            run("use snafu::prelude::*;\n#[derive(Debug, Snafu)]\npub enum Error { InvalidUrl { url: String } }")
                .is_empty(),
            "must not flag an enum deriving snafu's error library"
        );
    }

    #[test]
    fn allows_enum_with_miette() {
        assert!(
            run("use miette::Diagnostic;\n#[derive(Debug, Diagnostic, thiserror::Error)]\npub enum Error { #[error(\"boom\")] Boom }")
                .is_empty(),
            "must not flag an enum deriving miette's diagnostic/error library"
        );
    }

    /// Regression for #5288: the error-derive library may be declared as a crate
    /// dependency while the derive itself lives in a sibling file. The nearest
    /// `Cargo.toml` listing `snafu` exempts the crate even when this file's own
    /// source never names it.
    #[test]
    fn allows_pub_enum_error_in_snafu_dep_crate() {
        const SNAFU_DEP_CARGO_TOML: &str = r#"
[package]
name = "snafu-lib"
version = "0.1.0"
edition = "2021"

[dependencies]
snafu = "0.8"
"#;
        // The enum hand-rolls `Display` (so the impl-presence precondition
        // holds); the crate is exempt solely because the manifest depends on
        // snafu, which is what this test exercises.
        let src = "pub enum MyError { Fail }\n\
                   impl std::fmt::Display for MyError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"fail\") }\n\
                   }";
        assert!(
            run_on_with_cargo(SNAFU_DEP_CARGO_TOML, src).is_empty(),
            "must not flag pub error enums in a crate depending on snafu"
        );
    }

    /// Negative counterpart of #5288: a crate that hand-rolls its error type
    /// with no error-derive library at all — neither in source nor manifest —
    /// must still be flagged.
    #[test]
    fn still_flags_hand_rolled_error_without_error_derive_crate() {
        const PLAIN_CARGO_TOML: &str = r#"
[package]
name = "plain-lib"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#;
        let src = "pub enum MyError { Fail }\n\
                   impl std::fmt::Display for MyError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"fail\") }\n\
                   }\n\
                   impl std::error::Error for MyError {}";
        assert_eq!(
            run_on_with_cargo(PLAIN_CARGO_TOML, src).len(),
            1,
            "must keep flagging hand-rolled error types with no error-derive library"
        );
    }

    #[test]
    fn ignores_main_rs() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "pub enum MyError { Fail }", "src/main.rs");
        assert!(diags.is_empty());
    }

    /// Run the rule on `dir/<rel_path>` in a binary-only crate: the given
    /// `Cargo.toml` has no `[lib]` table and no `src/lib.rs` is created, so
    /// `CargoManifest::is_binary_only` returns true for the resolved manifest.
    fn run_in_binary_only_crate(
        cargo_toml_contents: &str,
        rel_path: &str,
        source: &str,
    ) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    const SHADER_ERROR_SRC: &str = "pub enum ShaderError { Compile(String), Link(String) }\n\
         impl std::error::Error for ShaderError {}";

    /// Regression for #6984 (alacritty): a binary-only application crate (no
    /// `[lib]` table, no `src/lib.rs`) has no downstream library consumers, so
    /// its error types are application-internal even in non-entry-point files
    /// like `src/renderer/shader.rs` — not just `main.rs` / `src/bin/`.
    #[test]
    fn allows_pub_error_in_binary_only_crate() {
        assert!(
            run_in_binary_only_crate(STD_CARGO_TOML, "src/renderer/shader.rs", SHADER_ERROR_SRC)
                .is_empty(),
            "must not flag error types in a binary-only crate"
        );
    }

    /// Negative counterpart of #6984: the same error type in a library crate
    /// (`run_on_with_cargo` creates `src/lib.rs`, so `is_binary_only` is false)
    /// is public library API and must still be flagged.
    #[test]
    fn still_flags_pub_error_in_library_crate() {
        assert_eq!(
            run_on_with_cargo(STD_CARGO_TOML, SHADER_ERROR_SRC).len(),
            1,
            "must keep flagging error types in a library crate"
        );
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
        // The enum hand-rolls `Display` (so the impl-presence precondition
        // holds); the crate is exempt solely because the manifest declares
        // `categories = ["no-std"]`, which is what this test exercises.
        let src = "pub enum MyError { Fail }\n\
                   impl core::fmt::Display for MyError {\n\
                       fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result { write!(f, \"fail\") }\n\
                   }";
        assert!(
            run_on_with_cargo(NO_STD_CARGO_TOML, src).is_empty(),
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
        let src = "pub enum MyError { Fail }\n\
                   impl std::fmt::Display for MyError {\n\
                       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"fail\") }\n\
                   }";
        assert_eq!(
            run_on_with_cargo(STD_CARGO_TOML, src).len(),
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
