//! rust-impl-debug-on-public-types backend.
//!
//! For every `struct_item` and `enum_item` with a strictly `pub` visibility
//! modifier, scan the preceding `attribute_item` siblings looking
//! for either `#[derive(...Debug...)]` or a manual `impl Debug for
//! ...` somewhere in the file. Flag if neither is present.
//!
//! Suppressed for: `pub(crate)`/`pub(super)`/`pub(in …)` visibility,
//! files under `tests/` or `benches/`, items in a `#[cfg(test)]` module,
//! items in a `proc-macro = true` crate (whose `pub` types are unreachable by
//! consumers), items with `#[doc(hidden)]`, items carrying
//! `#[allow(missing_debug_implementations)]` or
//! `#[expect(missing_debug_implementations)]` (the rustc lint this rule
//! mirrors — the author has explicitly opted out), and types with
//! raw-pointer fields.
//!
//! We accept manual impls because libraries with closure or PhantomData
//! fields legitimately can't derive — they hand-roll the impl. The
//! file-wide check is a heuristic but matches real codebases.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{has_doc_hidden, has_test_attribute, is_in_test_context};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["struct_item", "enum_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let source_str = ctx.source;
        let kind = node.kind();
        if !is_pub(node, source_bytes) {
            return;
        }
        if ctx.path.components().any(|c| {
            c.as_os_str() == "tests" || c.as_os_str() == "benches"
        }) {
            return;
        }
        if is_in_test_context(node, source_bytes) || has_test_attribute(node, source_bytes) {
            return;
        }
        // A `proc-macro = true` crate can export only procedural macros; its
        // `pub` types are unreachable by any consumer, so "consumers can't
        // debug it" is structurally inapplicable.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_proc_macro())
        {
            return;
        }
        if has_doc_hidden(node, source_bytes) {
            return;
        }
        if has_allow_missing_debug(node, source_bytes) {
            return;
        }
        if has_raw_pointer_field(node) {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if has_debug_derive(node, source_bytes) {
            return;
        }
        // Manual `impl Debug for Name` anywhere in the file.
        if source_str.contains(&format!("impl Debug for {name}"))
            || source_str.contains(&format!("impl std::fmt::Debug for {name}"))
            || source_str.contains(&format!("impl fmt::Debug for {name}"))
        {
            return;
        }
        let pos = node.start_position();
        let kind_label = if kind == "struct_item" {
            "struct"
        } else {
            "enum"
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-impl-debug-on-public-types".into(),
            message: format!(
                "`pub {kind_label} {name}` has no `Debug` impl — \
                 consumers can't log it, can't use it in assert \
                 failure messages, can't see it in `{{:?}}` output. \
                 Add `#[derive(Debug)]` or implement `Debug` by hand."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text == "pub"
        {
            return true;
        }
    }
    false
}

fn has_debug_derive(item: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk every preceding sibling; keep going through attribute_item
    // and comment nodes (both `line_comment` and `block_comment`, which
    // tree-sitter-rust inserts between attributes when a trailing `//`
    // or block comment sits beside an attribute like
    // `#[allow(...)] // trailing note`). Stop at the first sibling that
    // isn't an attribute or a comment — that's where our declaration's
    // attribute block actually ends.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && text.contains("derive(")
                    && text.contains("Debug")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Comments interleaved with attributes don't end the
                // attribute block. Keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if a preceding `attribute_item` sibling is
/// `#[allow(missing_debug_implementations)]` or
/// `#[expect(missing_debug_implementations)]`. That is the exact rustc lint
/// this rule mirrors, so an explicit allow/expect of it means the author has
/// deliberately opted out and we defer to that.
///
/// Walks preceding siblings like `has_doc_hidden`/`has_debug_derive`, skipping
/// interleaved comment siblings. The match is specific: the attribute path must
/// be `allow` or `expect` AND its argument list must contain
/// `missing_debug_implementations`, so an unrelated `#[allow(dead_code)]` does
/// not suppress.
fn has_allow_missing_debug(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if attribute_allows_lint(s, source, "missing_debug_implementations") {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `attribute_item` is an `allow`/`expect` attribute whose argument list
/// names `lint`, bare or tool-scoped (`rustc::<lint>`).
///
/// `attribute_item` parses as `attribute_item > attribute`, where the
/// `attribute` is `seq($._path, optional(arguments: token_tree))`: its first
/// named child is the path (`allow`/`expect`) and its arguments live in the
/// `token_tree` as a flat sequence of `identifier` tokens. We match on the AST
/// path child (`allow`/`expect`) and on the `identifier` tokens inside the
/// token tree rather than scanning raw text, so a lint merely ending in
/// `_missing_debug_implementations`, or the name appearing inside an unrelated
/// string, does not match. A tool-scoped `rustc::missing_debug_implementations`
/// still tokenizes the final segment as its own `identifier`, so it matches too.
fn attribute_allows_lint(attribute_item: tree_sitter::Node, source: &[u8], lint: &str) -> bool {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };

    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    let Ok(path_text) = path.utf8_text(source) else {
        return false;
    };
    if path_text != "allow" && path_text != "expect" {
        return false;
    }

    let Some(token_tree) = attribute.child_by_field_name("arguments") else {
        return false;
    };

    let mut tree_cursor = token_tree.walk();
    token_tree
        .children(&mut tree_cursor)
        .any(|tok| tok.kind() == "identifier" && tok.utf8_text(source) == Ok(lint))
}

fn has_raw_pointer_field(item: tree_sitter::Node) -> bool {
    let mut cursor = item.walk();
    loop {
        if cursor.node().kind() == "pointer_type" {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.node().id() == item.id() {
                return false;
            }
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    fn run_with_path(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, fake_path)
    }

    /// Run on a file in `dir/src/x.rs` next to the given `Cargo.toml`, so
    /// `nearest_cargo_manifest` resolves the temp crate's manifest (e.g. for
    /// the `proc-macro = true` exemption).
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join("src/x.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_pub_struct_without_debug() {
        assert_eq!(run_on("pub struct User { name: String }").len(), 1);
    }

    #[test]
    fn flags_pub_enum_without_debug() {
        assert_eq!(run_on("pub enum State { Idle, Busy }").len(), 1);
    }

    #[test]
    fn allows_pub_struct_with_debug_derive() {
        assert!(run_on("#[derive(Debug)]\npub struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_pub_struct_with_mixed_derive() {
        assert!(
            run_on("#[derive(Clone, Debug, Default)]\npub struct User { name: String }").is_empty()
        );
    }

    #[test]
    fn allows_pub_struct_with_manual_debug_impl() {
        let source = "pub struct Closure { f: Box<dyn Fn()> }\nimpl Debug for Closure { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_struct() {
        assert!(run_on("struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_doc_comment_above_multi_attribute_block() {
        // Reproduces the RuleMeta false positive: a doc comment, then
        // `#[derive(Debug, ...)]`, then another `#[allow(...)]`, then
        // the struct. The walker must traverse both attribute items
        // without being stopped by the preceding doc comment.
        let source = "/// Doc line 1.\n\
                      /// Doc line 2.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)]\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: multi-attribute block with Debug derive should not fire"
        );
    }

    #[test]
    fn suppresses_pub_crate_struct() {
        assert!(run_on("pub(crate) struct Internal { x: u8 }").is_empty());
    }

    #[test]
    fn suppresses_pub_struct_in_tests_dir() {
        assert!(run_with_path("pub struct X;", "tests/foo.rs").is_empty());
    }

    #[test]
    fn suppresses_pub_struct_in_benches_dir() {
        assert!(run_with_path("pub struct X;", "benches/bench.rs").is_empty());
    }

    #[test]
    fn suppresses_doc_hidden_enum() {
        assert!(run_on("#[doc(hidden)]\npub enum Y {}").is_empty());
    }

    #[test]
    fn suppresses_cfg_test_struct() {
        assert!(run_on("#[cfg(test)]\npub struct Z;").is_empty());
    }

    #[test]
    fn suppresses_struct_inside_cfg_test_mod() {
        assert!(run_on("#[cfg(test)]\nmod tests {\n    pub struct TestHelper;\n}").is_empty());
    }

    #[test]
    fn suppresses_raw_pointer_field() {
        assert!(run_on("pub struct W { p: *const u8 }").is_empty());
    }

    #[test]
    fn still_flags_plain_pub_struct() {
        assert_eq!(run_on("pub struct Api { name: String }").len(), 1);
    }

    #[test]
    fn suppresses_allow_missing_debug_implementations() {
        // tokio net/addr.rs:269-270 — author explicitly opted out of the
        // rustc lint this rule mirrors.
        assert!(
            run_on("#[allow(missing_debug_implementations)]\npub struct Internal;").is_empty()
        );
    }

    #[test]
    fn suppresses_expect_missing_debug_implementations() {
        assert!(run_on("#[expect(missing_debug_implementations)]\npub struct X;").is_empty());
    }

    #[test]
    fn suppresses_allow_missing_debug_with_interleaved_cfg_attr() {
        // tokio runtime/task_hooks.rs:57-59 — a `#[cfg_attr(...)]` sits
        // between the allow and the item; the walk must traverse it.
        let source = "#[allow(missing_debug_implementations)]\n\
                      #[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]\n\
                      pub struct TaskMeta<'a> { id: &'a str }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_tool_scoped_allow_missing_debug() {
        // rustc accepts a tool-scoped lint path; the final segment still
        // tokenizes as a bare identifier inside the token tree.
        assert!(
            run_on("#[allow(rustc::missing_debug_implementations)]\npub struct X;").is_empty()
        );
    }

    #[test]
    fn still_flags_with_unrelated_allow() {
        // `#[allow(dead_code)]` is unrelated; suppression is lint-specific.
        assert_eq!(run_on("#[allow(dead_code)]\npub struct X { name: String }").len(), 1);
    }

    const PROC_MACRO_CARGO_TOML: &str = r#"
[package]
name = "prost-derive-like"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "normal-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "normal_lib"
"#;

    /// Closes #3960: a `pub` type in a `proc-macro = true` crate is unreachable
    /// by any consumer (prost-derive `field/scalar.rs:585` etc.), so it must
    /// not be flagged.
    #[test]
    fn suppresses_pub_type_in_proc_macro_crate() {
        assert!(
            run_on_with_cargo(PROC_MACRO_CARGO_TOML, "pub enum Kind { Bool, Int }").is_empty(),
            "must not flag pub types in a proc-macro crate"
        );
        assert!(
            run_on_with_cargo(PROC_MACRO_CARGO_TOML, "pub struct Field { name: String }")
                .is_empty(),
            "must not flag pub structs in a proc-macro crate"
        );
    }

    /// A normal library crate (`[lib]` without `proc-macro = true`) exposes its
    /// `pub` types to consumers — the exemption is proc-macro-only, so it must
    /// still flag.
    #[test]
    fn still_flags_pub_type_in_normal_lib_crate() {
        assert_eq!(
            run_on_with_cargo(LIB_CARGO_TOML, "pub struct Api { name: String }").len(),
            1,
            "a normal lib crate's pub type with no Debug must still flag"
        );
    }

    #[test]
    fn allows_trailing_comment_after_inner_attribute() {
        // Reproduces the exact RuleMeta shape in meta.rs: a trailing
        // `// comment` after `#[allow(dead_code)]` between the derive
        // and the struct. tree-sitter-rust may split this differently.
        let source = "/// Doc.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)] // Fields read by JSON output / explain / remap (coming soon).\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: trailing line comment after attribute should not defeat Debug detection"
        );
    }
}
