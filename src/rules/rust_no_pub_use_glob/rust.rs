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
//!   stays inside the crate.

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
        // targets. (External / cross-module globs like `pub use serde::*;` or
        // `pub use crate::types::*;` are not exempt — no local `mod` matches.)
        if let Some(seg) = first_use_segment(trimmed) {
            if declares_submodule(node, seg, source_bytes) {
                return;
            }
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

/// First path segment of a `pub use` target, skipping a leading `self::`.
/// `pub use execution_state::*;` -> `Some("execution_state")`
/// `pub use self::foo::*;`       -> `Some("foo")`
/// `pub use crate::types::*;`    -> `Some("crate")`
fn first_use_segment(trimmed: &str) -> Option<&str> {
    let after = trimmed.strip_prefix("pub use")?.trim_start();
    let after = after.strip_prefix("self::").unwrap_or(after);
    let seg = after.split("::").next()?.trim();
    (!seg.is_empty()).then_some(seg)
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

    #[test]
    fn flags_pub_use_glob() {
        assert_eq!(run_on("pub use crate::types::*;").len(), 1);
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
    fn still_flags_external_crate_glob_issue_1013() {
        // `serde` is an external crate, not a submodule declared here.
        assert_eq!(run_on("pub use serde::*;").len(), 1);
    }

    #[test]
    fn still_flags_bare_glob_without_local_mod() {
        // No `mod external_thing;` in the file -> not local flattening.
        assert_eq!(run_on("pub use external_thing::*;").len(), 1);
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
        // the public API -> still flagged.
        let src = "#[cfg(feature = \"derive\")]\npub use serde::*;";
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
            "pub use self::platform::*;\n",
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
            "pub use self::platform::*;\n",
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
            "pub use self::platform::*;\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }
}
