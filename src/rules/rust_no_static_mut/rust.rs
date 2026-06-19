//! rust-no-static-mut backend.
//!
//! Flags `static mut FOO: T = ...` declarations. The Rust 2024
//! edition deprecates this feature because every read or write
//! requires `unsafe` and there's no race-free path to use it
//! correctly without wrapping in a sync primitive — at which point
//! you might as well use the sync primitive directly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["static_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // tree-sitter-rust represents `static mut FOO` by including
        // a `mutable_specifier` child holding the `mut` keyword.
        let mut cursor = node.walk();
        let has_mut = node
            .children(&mut cursor)
            .any(|c| c.kind() == "mutable_specifier");
        if !has_mut {
            return;
        }
        // `no_std` exemption: `OnceLock`/`LazyLock`/`Mutex`/`RwLock`/`Atomic*`
        // live in `std`, not `core`. In a `no_std` crate (bare-metal, embedded)
        // `static mut` is the only mechanism available for hardware singletons,
        // interrupt state and MMIO addresses — the suggested alternatives don't
        // compile. Skip when this file declares a `#![no_std]` inner attribute,
        // the crate's manifest is categorized no-std, or the crate root declares
        // `#![no_std]` (the attribute usually lives in `lib.rs`/`main.rs`, not
        // the flagged file).
        if crate::project::source_declares_no_std(ctx.source)
            || ctx
                .project
                .nearest_cargo_manifest(ctx.path)
                .is_some_and(|m| m.is_no_std())
            || ctx.project.crate_root_is_no_std(ctx.path)
        {
            return;
        }
        // Surface the static's name in the message if we can read it.
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("FOO");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-static-mut".into(),
            message: format!(
                "`static mut {name}` — deprecated in Rust 2024 and \
                 impossible to use race-free. Use `OnceLock`/`LazyLock` \
                 for init-once, `Mutex`/`RwLock` for shared state, or \
                 `Atomic*` for primitive counters."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
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
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    /// Build a crate on disk so the `no_std` exemptions resolve against real
    /// files: `Cargo.toml`, a crate root (`src/main.rs`), and `src/foo.rs`
    /// holding the source under test. The rule runs on `foo.rs`.
    fn run_in_crate(cargo_toml: &str, crate_root: &str, foo_src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), crate_root).unwrap();
        let foo_path = dir.path().join("src/foo.rs");
        fs::write(&foo_path, foo_src).unwrap();
        crate::rules::test_helpers::run_rule(&Check, foo_src, &foo_path)
    }

    const STD_CARGO_TOML: &str = "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    const NO_STD_CARGO_TOML: &str =
        "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\ncategories = [\"no-std\"]\n";

    #[test]
    fn flags_static_mut() {
        assert_eq!(run_on("static mut COUNTER: u64 = 0;").len(), 1);
    }

    #[test]
    fn allows_static_immutable() {
        assert!(run_on("static MAX: u32 = 100;").is_empty());
    }

    #[test]
    fn allows_const() {
        assert!(run_on("const MAX: u32 = 100;").is_empty());
    }

    /// The text `no_std` in a comment must not exempt the file: it is not a
    /// `#![no_std]` declaration, so a real `static mut` in a `std` file is still
    /// flagged (regression for #4021 — the old substring guard over-suppressed).
    #[test]
    fn still_flags_static_mut_when_no_std_only_in_comment() {
        assert_eq!(
            run_on("// works in no_std too\nstatic mut COUNTER: usize = 0;").len(),
            1,
            "`no_std` in a comment must not exempt a real `static mut`"
        );
    }

    /// The text `no_std` in an identifier must not exempt the file either.
    #[test]
    fn still_flags_static_mut_when_no_std_only_in_identifier() {
        assert_eq!(
            run_on("fn supports_no_std() {}\nstatic mut X: u8 = 0;").len(),
            1,
            "`no_std` in an identifier must not exempt a real `static mut`"
        );
    }

    // ── no_std exemptions (Closes #1331) ──────────────────────────────────

    /// The flagged file itself declares `#![no_std]` → `std` sync primitives
    /// are unavailable, so `static mut` is exempt.
    #[test]
    fn allows_static_mut_in_no_std_source() {
        assert!(
            run_on("#![no_std]\npub static mut X: usize = 0;").is_empty(),
            "must not flag `static mut` in a #![no_std] file"
        );
    }

    /// The crate root (`main.rs`/`lib.rs`) declares `#![no_std]` even though the
    /// flagged file (`foo.rs`) does not — mirrors the issue's xous-core example.
    #[test]
    fn allows_static_mut_when_crate_root_is_no_std() {
        assert!(
            run_in_crate(STD_CARGO_TOML, "#![no_std]\nfn main() {}", "pub static mut X: usize = 0;")
                .is_empty(),
            "must not flag `static mut` when the crate root declares #![no_std]"
        );
    }

    /// The conditional `#![cfg_attr(not(test), no_std)]` form in the crate root.
    #[test]
    fn allows_static_mut_when_crate_root_is_conditionally_no_std() {
        assert!(
            run_in_crate(
                STD_CARGO_TOML,
                "#![cfg_attr(not(test), no_std)]\nfn main() {}",
                "pub static mut X: usize = 0;"
            )
            .is_empty(),
            "must not flag `static mut` under #![cfg_attr(not(test), no_std)]"
        );
    }

    /// The crate's `Cargo.toml` lists the `no-std` category.
    #[test]
    fn allows_static_mut_in_no_std_category_crate() {
        assert!(
            run_in_crate(NO_STD_CARGO_TOML, "fn main() {}", "pub static mut X: usize = 0;")
                .is_empty(),
            "must not flag `static mut` in a crate with the no-std category"
        );
    }

    /// Negative space: an ordinary `std` crate with no `no_std` signal anywhere
    /// must still be flagged.
    #[test]
    fn still_flags_static_mut_in_std_crate() {
        assert_eq!(
            run_in_crate(STD_CARGO_TOML, "fn main() {}", "pub static mut X: usize = 0;").len(),
            1,
            "must keep flagging `static mut` in ordinary std crates"
        );
    }
}
