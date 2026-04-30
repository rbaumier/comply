//! rust-constants-top-of-file backend.
//!
//! Scans only the direct children of the `source_file` node. We find the
//! first "blocking item" (function / struct / enum / impl / trait / mod /
//! type alias / union) and then emit a diagnostic for every `const_item`
//! or `static_item` that appears after it at the same top level.
//!
//! `use` declarations and `extern crate` items are not considered
//! blocking — they can be interleaved freely with constants. Constants
//! nested inside `impl` blocks, function bodies, or submodules are not
//! at module level and are therefore ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Node kinds that, once seen at module level, mean subsequent
/// module-level constants are "buried" and should be flagged.
const BLOCKING_ITEM_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "impl_item",
    "trait_item",
    "type_item",
    "union_item",
];

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let root = tree.root_node();
        if root.kind() != "source_file" {
            return diagnostics;
        }

        let mut cursor = root.walk();
        let mut seen_blocking_item = false;
        for child in root.named_children(&mut cursor) {
            let kind = child.kind();
            if BLOCKING_ITEM_KINDS.contains(&kind) {
                seen_blocking_item = true;
                continue;
            }
            // `mod foo;` (no body) is a declaration like `use` — not blocking.
            // `mod foo { ... }` (with body) is blocking.
            if kind == "mod_item" && child.child_by_field_name("body").is_some() {
                seen_blocking_item = true;
                continue;
            }
            if !seen_blocking_item {
                continue;
            }
            if kind != "const_item" && kind != "static_item" {
                continue;
            }
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-constants-top-of-file".into(),
                message: "Module-level `const` / `static` should appear at the top of the \
                          file, before any `fn` / `struct` / `impl`. Readers scanning a \
                          file expect to find configuration constants and thresholds up \
                          front, not buried between functions."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_const_after_fn() {
        let diags = run_on("fn f() {}\nconst C: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_static_after_struct() {
        let diags = run_on("struct S;\nstatic X: u32 = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_const_before_fn() {
        assert!(run_on("const C: u32 = 1;\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_multiple_consts_before_fn() {
        let source = "const A: u32 = 1;\nconst B: u32 = 2;\nfn f() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_const_inside_impl() {
        let source = "struct S;\nimpl S { const A: u32 = 1; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_const_inside_fn() {
        assert!(run_on("fn f() { const A: u32 = 1; }").is_empty());
    }

    #[test]
    fn allows_use_before_const() {
        let source = "use std::fmt;\nconst C: u32 = 1;\nfn f() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_use_between_const_and_fn() {
        let source = "const C: u32 = 1;\nuse std::fmt;\nfn f() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_const_after_mod_declaration() {
        let source = "mod sub;\nconst C: u32 = 1;\nfn f() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_const_after_mod_block() {
        let source = "mod sub { fn inner() {} }\nconst C: u32 = 1;";
        let diags = run_on(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }
}
