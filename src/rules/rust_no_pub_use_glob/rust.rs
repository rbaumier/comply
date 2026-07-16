//! rust-no-pub-use-glob backend.
//!
//! Walks `use_declaration` nodes whose source text starts with `pub`
//! and ends with `*;`. We use the textual form rather than the AST
//! because the wildcard is represented as a `use_wildcard` node
//! deep in the tree, and the `pub` modifier is a separate child —
//! easier to scan the line.
//!
//! Cases that are exempt because they do not invisibly mirror an external
//! dependency's surface:
//! - prelude modules (`prelude.rs` / `prelude/mod.rs`), which exist
//!   precisely to be glob-imported (`use my_crate::prelude::*`);
//! - local-submodule flattening (`mod foo; pub use foo::*;`), which
//!   re-exports a submodule the author owns in the same file;
//! - a `pub use` confined to a non-public module — whether inline
//!   (`mod foo { pub use ...::*; }`) or split-file (the flagged file's `mod foo;`
//!   declaration is non-public in its parent file) — since effective visibility
//!   stays inside the crate;
//! - a glob re-export of a proc-macro companion crate (`pub use serde_derive::*;`,
//!   `pub use thiserror_impl::*;`), identified by the `_derive` / `_impl` / `_macros`
//!   naming convention — the only way a parent crate can expose a `proc-macro = true`
//!   crate's derive macro, and a documented part of its public API;
//! - an umbrella/facade crate re-exporting one of its own Cargo-family sub-crates
//!   (`pub use salvo_core::*;` in package `salvo`, `pub use poem_core::*;` in `poem`),
//!   identified by the glob source starting with `<package_name>_` — wholesale
//!   re-export of the core sub-crate IS the umbrella crate's public API;
//! - a file whose sole top-level item (ignoring comments and attributes) is the
//!   flagged `pub use ...::*;` itself — a single-statement re-export facade whose
//!   entire public API is the wholesale re-export (e.g. a thin `errors` crate
//!   that is just `pub use anyhow::*;`). A second top-level item makes the file
//!   mix the glob with its own surface, so it stays flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use std::path::Path;

const KINDS: &[&str] = &["use_declaration"];

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
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        // Strip leading whitespace, check `pub use … *;` shape.
        let trimmed = text.trim_start();
        if !trimmed.starts_with("pub use") && !trimmed.starts_with("pub(") {
            return;
        }
        // The `pub(crate)` form is OK — we only complain about the
        // truly public `pub use`. Detect by checking for `pub use ` exactly
        // OR `pub use ` after a `pub(scope)` modifier.
        if trimmed.starts_with("pub(crate)") || trimmed.starts_with("pub(super)") {
            return;
        }
        // A bare `pub use` confined to a non-public module (`pub(crate) mod foo`,
        // a private `mod foo`) cannot reach the crate's public API: effective
        // visibility is the product of the item modifier and every enclosing
        // module's. The "your crate's API quietly mirrors theirs" rationale is
        // false there, so it is exempt just like a directly-written `pub(crate)`.
        // The first check handles inline modules (`mod foo { pub use ...::*; }`);
        // the second handles split-file modules, where the `mod foo;` declaration
        // is non-public in the parent file on disk (e.g. `mod platform_impl;`).
        if crate::rules::rust_helpers::is_inside_non_public_module(node, source_bytes)
            || ctx
                .project
                .rust_module_declared_private_in_parent(ctx.path)
        {
            return;
        }
        // Must end with the wildcard import.
        if !trimmed
            .trim_end()
            .trim_end_matches(';')
            .trim_end()
            .ends_with("::*")
        {
            return;
        }
        // `#[doc(hidden)]` removes the re-export from the documented public
        // surface, so it cannot "quietly mirror" a dependency's API — the
        // author has marked it internal plumbing. This is the canonical derive
        // companion-crate pattern (`#[doc(hidden)] pub use foo_derive::*;`, as
        // serde/thiserror/prost do). The attribute may sit beside `#[cfg(...)]`,
        // which the helper traverses past.
        if crate::rules::rust_helpers::has_doc_hidden(node, source_bytes) {
            return;
        }
        // Prelude modules exist to be glob-imported (`use crate::prelude::*`,
        // like `std::prelude`); wholesale re-export is their purpose.
        if is_prelude_module(ctx.path) {
            return;
        }
        // Module-flattening: `mod foo; pub use foo::*;` re-exports a submodule
        // the author owns in this same file to keep file layout separate from
        // the public API shape — not the dependency-surface mirroring this rule
        // targets. A leading `self::`/`crate::` names the same crate-local module,
        // so `pub use crate::foo::*;` flattens the local `mod foo;` identically.
        // (External globs like `pub use serde::*;` are not exempt — no local
        // `mod` matches.)
        // The same first segment also identifies a proc-macro companion crate
        // (`pub use thiserror_impl::*;`), the only way a parent crate can expose
        // a `proc-macro = true` crate's derive macro — see the helper below.
        if let Some(seg) = first_use_segment(trimmed) {
            if declares_submodule(node, seg, source_bytes)
                || is_proc_macro_companion_crate(seg)
                || ctx
                    .project
                    .nearest_cargo_manifest(ctx.path)
                    .is_some_and(|m| m.is_own_family_subcrate(seg))
            {
                return;
            }
        }
        // A file whose only top-level item (ignoring comments and attributes) is
        // this `pub use ...::*;` is a deliberate single-statement re-export
        // facade: its entire public API IS the wholesale re-export (e.g. a shared
        // `errors` crate that is just `pub use anyhow::*;`). There is no other
        // surface for the glob to silently mirror alongside. A second top-level
        // item (a `struct`, a `fn`, a second `use`, a `mod`, …) makes the count
        // exceed one and the file is no longer a pure facade, so it stays flagged.
        if is_sole_top_level_item(node) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-pub-use-glob".into(),
            message: "`pub use ...::*` re-exports every public symbol \
                      from the source module — your crate's API \
                      quietly mirrors theirs. List the names explicitly: \
                      `pub use foo::{Bar, Baz};`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// `prelude` modules (`prelude.rs` or `prelude/mod.rs`) exist precisely to
/// be glob-imported (`use my_crate::prelude::*`), the same convention as
/// `std::prelude`. Re-exporting wholesale is their entire purpose, so a
/// `pub use ...::*;` there is never a surprise.
fn is_prelude_module(path: &Path) -> bool {
    let file = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if file == "prelude.rs" {
        return true;
    }
    if file == "mod.rs" {
        return path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some("prelude");
    }
    false
}

/// First path segment of a `pub use` target, skipping a leading `self::` or
/// `crate::`. Both roots name the current crate, so stripping either yields the
/// crate-local module path — `pub use self::foo::*;` and `pub use crate::foo::*;`
/// both flatten the same owned `mod foo;`.
/// `pub use execution_state::*;` -> `Some("execution_state")`
/// `pub use self::foo::*;`       -> `Some("foo")`
/// `pub use crate::foo::*;`      -> `Some("foo")`
fn first_use_segment(trimmed: &str) -> Option<&str> {
    let after = trimmed.strip_prefix("pub use")?.trim_start();
    let after = after
        .strip_prefix("self::")
        .or_else(|| after.strip_prefix("crate::"))
        .unwrap_or(after);
    let seg = after.split("::").next()?.trim();
    (!seg.is_empty()).then_some(seg)
}

/// True when the flagged `use_declaration` is the entire content of its file:
/// it is a direct child of the `source_file` root and that root holds exactly
/// one item that is not a comment or attribute. Such a file — whose sole
/// statement is `pub use <dep>::*;`, optionally under a license comment
/// (`line_comment` / `block_comment`) or a crate-level attribute
/// (`attribute_item` / `inner_attribute_item`) — is a deliberate
/// single-statement re-export facade. Any real second item (`function_item`,
/// `struct_item`, `mod_item`, a second `use_declaration`, …) pushes the count
/// past one, so the file is no longer a pure facade.
fn is_sole_top_level_item(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "source_file" {
        return false;
    }
    let mut cursor = parent.walk();
    parent
        .named_children(&mut cursor)
        .filter(|child| {
            !matches!(
                child.kind(),
                "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item"
            )
        })
        .count()
        == 1
}

/// True when `crate_name` follows the conventional naming of a proc-macro /
/// derive companion crate (`serde_derive`, `thiserror_impl`, `async_trait_impl`,
/// `*_macros`). A `proc-macro = true` crate can export only procedural macros, so
/// the parent crate re-exports them with `pub use <impl_crate>::*` — the
/// universal, documented derive-library pattern, not a hidden API mirror.
fn is_proc_macro_companion_crate(crate_name: &str) -> bool {
    crate_name.ends_with("_derive")
        || crate_name.ends_with("_impl")
        || crate_name.ends_with("_macros")
}

/// True if the file declares a submodule named `seg` (`mod seg;` or
/// `mod seg { ... }`). Then `pub use seg::*;` flattens a submodule the
/// author owns in this very file, an intentional API-shape choice.
fn declares_submodule(node: tree_sitter::Node, seg: &str, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    find_mod_decl(root, seg, source)
}

fn find_mod_decl(node: tree_sitter::Node, seg: &str, source: &[u8]) -> bool {
    if node.kind() == "mod_item"
        && node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            == Some(seg)
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if find_mod_decl(child, seg, source) {
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    /// Write `parent_rel` and `child_rel` into a temp crate, then run the rule
    /// on the child so `rust_module_declared_private_in_parent` can read the
    /// parent's `mod` declaration off disk.
    fn run_split_module(
        parent_rel: &str,
        parent_src: &str,
        child_rel: &str,
        child_src: &str,
    ) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        for rel in [parent_rel, child_rel] {
            if let Some(parent) = std::path::Path::new(rel).parent() {
                fs::create_dir_all(dir.path().join(parent)).unwrap();
            }
        }
        fs::write(dir.path().join(parent_rel), parent_src).unwrap();
        let child_path = dir.path().join(child_rel);
        fs::write(&child_path, child_src).unwrap();
        crate::rules::test_helpers::run_rule(&Check, child_src, &child_path)
    }

    /// Build a crate on disk so `nearest_cargo_manifest` resolves the package
    /// name from a real `Cargo.toml`. Writes the manifest with `name = pkg`, a
    /// crate root (`src/lib.rs`), and `src/foo.rs` holding the source under test;
    /// the rule runs on `foo.rs`.
    fn run_in_crate(pkg: &str, foo_src: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        let foo_path = dir.path().join("src/foo.rs");
        fs::write(&foo_path, foo_src).unwrap();
        crate::rules::test_helpers::run_rule(&Check, foo_src, &foo_path)
    }

    #[test]
    fn flags_pub_use_glob() {
        // A second top-level item keeps the file out of the single-statement
        // facade exemption, so the glob re-export is still flagged.
        assert_eq!(run_on("pub use crate::types::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn exempts_sole_item_facade_issue_6563() {
        // Issue #6563: getzola/zola components/errors/src/lib.rs is a thin facade
        // crate whose entire content is `pub use anyhow::*;`. With the glob as the
        // file's only item, the wholesale re-export IS the public API.
        assert!(run_on("pub use anyhow::*;").is_empty());
    }

    #[test]
    fn exempts_sole_item_facade_with_license_comment_issue_6563() {
        // A leading license comment is not a real item — the `pub use` is still
        // the sole top-level item, so the facade exemption holds.
        let src = "// SPDX-License-Identifier: MIT\npub use anyhow::*;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn exempts_sole_item_facade_with_crate_attribute_issue_6563() {
        // A crate-level inner attribute (`#![...]`) is not a real item either, so
        // the `pub use` remains the sole top-level item -> facade exemption holds.
        let src = "#![allow(unused_imports)]\npub use anyhow::*;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_facade_plus_other_item_issue_6563() {
        // `pub use anyhow::*;` alongside another item is no longer a pure facade:
        // the glob mirrors the dependency's surface next to the crate's own API.
        let src = "pub use anyhow::*;\npub struct Foo;";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_two_pub_use_globs_issue_6563() {
        // Two top-level `pub use ...::*;` items push the count to two, so neither
        // is the sole item -> both stay flagged.
        assert_eq!(run_on("pub use x::*;\npub use y::*;").len(), 2);
    }

    #[test]
    fn exempts_umbrella_family_subcrate_glob_issue_4461() {
        // Issue #4461: salvo-rs/salvo — the umbrella `salvo` crate re-exports its
        // own core sub-crate's entire public API via `pub use salvo_core::*;`.
        assert!(run_in_crate("salvo", "pub use salvo_core::*;").is_empty());
    }

    #[test]
    fn exempts_umbrella_family_extra_subcrate_glob_issue_4461() {
        // Other family sub-crates of the same umbrella (`salvo_extra`) too.
        assert!(run_in_crate("salvo", "pub use salvo_extra::*;").is_empty());
    }

    #[test]
    fn still_flags_family_subcrate_glob_in_unrelated_crate_issue_4461() {
        // The exemption is package-relative, not a blanket `*_core` suffix: a crate
        // named `othercrate` re-exporting `salvo_core::*` is still mirroring an
        // external dependency's surface -> flagged.
        assert_eq!(
            run_in_crate("othercrate", "pub use salvo_core::*;\npub struct Marker;").len(),
            1
        );
    }

    #[test]
    fn still_flags_external_crate_glob_in_umbrella_crate_issue_4461() {
        // `serde` is not a `salvo_*` family sub-crate, so package `salvo` re-exporting
        // `serde::*` does mirror an external dependency -> flagged.
        assert_eq!(
            run_in_crate("salvo", "pub use serde::*;\npub struct Marker;").len(),
            1
        );
    }

    #[test]
    fn exempts_proc_macro_companion_thiserror_impl_issue_4510() {
        // Issue #4510: thiserror re-exports its `proc-macro = true` impl crate via
        // `pub use thiserror_impl::*;` (no `#[doc(hidden)]`) — the documented way to
        // expose the derive macro, not a hidden API mirror.
        assert!(run_on("pub use thiserror_impl::*;").is_empty());
    }

    #[test]
    fn exempts_proc_macro_companion_serde_derive_issue_4510() {
        // serde's bridge: `pub use serde_derive::*;`.
        assert!(run_on("pub use serde_derive::*;").is_empty());
    }

    #[test]
    fn exempts_proc_macro_companion_async_trait_impl_issue_4510() {
        assert!(run_on("pub use async_trait_impl::*;").is_empty());
    }

    #[test]
    fn exempts_proc_macro_companion_macros_suffix_issue_4510() {
        assert!(run_on("pub use foo_macros::*;").is_empty());
    }

    #[test]
    fn still_flags_external_crate_without_companion_suffix_issue_4510() {
        // No `_derive` / `_impl` / `_macros` suffix -> a normal glob re-export that
        // does mirror the dependency's surface. The exemption must stay suffix-gated.
        assert_eq!(run_on("pub use some_external_lib::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn still_flags_std_collections_glob_issue_4510() {
        // First segment `std` has no companion suffix -> still flagged.
        assert_eq!(run_on("pub use std::collections::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn allows_pub_use_explicit_list() {
        assert!(run_on("pub use crate::types::{Foo, Bar};").is_empty());
    }

    #[test]
    fn allows_private_use_glob() {
        assert!(run_on("use crate::types::*;").is_empty());
    }

    #[test]
    fn allows_pub_crate_use_glob() {
        // pub(crate) doesn't escape the crate — internal scope, fine.
        assert!(run_on("pub(crate) use crate::types::*;").is_empty());
    }

    #[test]
    fn exempts_prelude_file_issue_1013() {
        // Issue #1013: polars crates/*/src/prelude.rs glob re-exports.
        let src = "pub use crate::expressions::*;\npub use crate::state::*;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "src/prelude.rs").is_empty());
    }

    #[test]
    fn exempts_prelude_mod_rs() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "pub use crate::types::*;", "prelude/mod.rs")
                .is_empty()
        );
    }

    #[test]
    fn exempts_local_submodule_flattening_issue_1013() {
        // Issue #1013: polars state/mod.rs flattens an owned submodule.
        let src = "mod execution_state;\nmod node_timer;\npub use execution_state::*;\nuse node_timer::*;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn exempts_self_prefixed_submodule_flattening() {
        assert!(run_on("mod foo;\npub use self::foo::*;").is_empty());
    }

    #[test]
    fn exempts_crate_prefixed_submodule_flattening_issue_7515() {
        // Issue #7515: databend src/query/expression/src/lib.rs flattens owned
        // submodules with an explicit `crate::` prefix. `aggregate`/`block` are
        // declared as `pub mod`/`mod` in this same file, so `pub use crate::…::*;`
        // is local flattening, identical to the bare/`self::` forms.
        let src = "pub mod aggregate;\nmod block;\n\
                   pub use crate::aggregate::*;\npub use crate::block::*;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_crate_prefixed_glob_without_local_mod_issue_7515() {
        // `crate::types` with no `mod types;` in this file is not local
        // flattening — stripping the `crate::` prefix must not blanket-exempt
        // every `crate::` glob.
        assert_eq!(run_on("pub use crate::types::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn still_flags_external_crate_multi_segment_glob_issue_7515() {
        // An external dependency's module (`databend_common_column::bitmap`) is
        // not declared in this file, so it stays flagged — broadening the prefix
        // strip must not weaken real dependency-surface mirroring.
        assert_eq!(
            run_on("pub use databend_common_column::bitmap::*;\npub struct Marker;").len(),
            1
        );
    }

    #[test]
    fn still_flags_external_crate_glob_issue_1013() {
        // `serde` is an external crate, not a submodule declared here.
        assert_eq!(run_on("pub use serde::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn still_flags_bare_glob_without_local_mod() {
        // No `mod external_thing;` in the file -> not local flattening.
        assert_eq!(run_on("pub use external_thing::*;\npub struct Marker;").len(), 1);
    }

    #[test]
    fn exempts_doc_hidden_glob_issue_3961() {
        // Issue #3961: prost re-exports its derive companion crate via a
        // `#[doc(hidden)]` glob — excluded from the documented public surface.
        assert!(run_on("#[doc(hidden)]\npub use prost_derive::*;").is_empty());
    }

    #[test]
    fn exempts_doc_hidden_glob_beside_cfg_issue_3961() {
        // The exact prost shape: `#[cfg(...)]` interleaved before the
        // `#[doc(hidden)]`. The exemption must traverse past the cfg.
        let src = "#[cfg(feature = \"derive\")]\n#[doc(hidden)]\npub use prost_derive::*;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_cfg_only_glob_without_doc_hidden() {
        // A `#[cfg(...)]` attribute alone does not remove the re-export from
        // the public API -> still flagged. The struct keeps the real-item count
        // above one (the attribute itself does not count) so the facade
        // exemption does not apply.
        let src = "#[cfg(feature = \"derive\")]\npub use serde::*;\npub struct Marker;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_pub_use_glob_in_pub_crate_module_issue_3864() {
        // Issue #3864: tokio src/loom/mocked.rs — a bare `pub use` inside a
        // `pub(crate) mod` cannot reach the crate's public API.
        let src = "pub(crate) mod thread {\n    pub use loom::thread::*;\n}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn exempts_pub_use_glob_in_private_module_issue_3864() {
        // A plain `mod foo` (no visibility modifier) is private; the re-export
        // stays inside the module and never escapes the crate.
        let src = "mod private_mod {\n    pub use foo::*;\n}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_pub_use_glob_in_pub_module_issue_3864() {
        // A bare-`pub` enclosing module keeps the effective visibility public,
        // so the re-export does reach the crate's API -> still flagged.
        let src = "pub mod public_mod {\n    pub use foo::*;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_pub_use_glob_in_split_file_private_mod_rs_issue_4501() {
        // Issue #4501: winit src/platform_impl/mod.rs — the parent declares the
        // module privately (`mod platform_impl;`), so the glob never reaches the
        // crate's public API even though it sits in a standalone file.
        let diags = run_split_module(
            "src/lib.rs",
            "mod platform_impl;\n",
            "src/platform_impl/mod.rs",
            "#[allow(unused_imports)]\npub use self::platform::*;\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn exempts_pub_use_glob_in_split_file_private_named_module_issue_4501() {
        // The `<name>.rs` form: parent declares `mod platform;` privately, child
        // is the sibling file `src/platform.rs`.
        let diags = run_split_module(
            "src/lib.rs",
            "mod platform;\n",
            "src/platform.rs",
            "pub use foo::*;\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn still_flags_pub_use_glob_in_split_file_public_module_issue_4501() {
        // A `pub mod platform_impl;` parent keeps the module public, so the glob
        // reaches the crate's API -> still flagged.
        let diags = run_split_module(
            "src/lib.rs",
            "pub mod platform_impl;\n",
            "src/platform_impl/mod.rs",
            "pub use self::platform::*;\npub struct Marker;\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_pub_use_glob_when_parent_declaration_absent_issue_4501() {
        // No parent file declares the module — privacy cannot be proven, so the
        // rule conservatively keeps flagging.
        let diags = run_split_module(
            "src/unrelated.rs",
            "// no mod declaration here\n",
            "src/platform_impl/mod.rs",
            "pub use self::platform::*;\npub struct Marker;\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_pub_use_glob_at_crate_root_issue_4501() {
        // A crate root (`lib.rs`) has no parent module, so a `pub use *` there is
        // always part of the public API -> still flagged.
        let diags = run_split_module(
            "src/other.rs",
            "// sibling file, irrelevant\n",
            "src/lib.rs",
            "pub use self::platform::*;\npub struct Marker;\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }
}
