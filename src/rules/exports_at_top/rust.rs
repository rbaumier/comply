//! exports-at-top backend for Rust.
//!
//! Rust semantics: `pub` items (functions, structs, enums, traits, consts)
//! should appear before private ones at module scope. A reader opening the
//! file should see the public API at the top.
//!
//! **Intentionally excluded from the "item" set:** `mod` declarations and
//! `use` imports. Those are infrastructure and the Rust idiom places them
//! at the top of the file regardless of visibility (`mod foo;` above
//! `pub use foo::Bar;` is the canonical shape). Counting them as "private
//! items" would fire on every single file that follows idiomatic Rust
//! module layout.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Item kinds at module scope that have a meaningful visibility for the
/// "API first, helpers below" rule. Intentionally EXCLUDED:
///
/// - `mod_item` / `use_declaration` — infrastructure; always at the top.
/// - `const_item` / `static_item` — module-level data/configuration,
///   not API; commonly lives near the imports, not after the pub fns.
/// - `impl_item` — inherent `impl` blocks have no direct visibility
///   (they adopt the type's) and trait impls are driven by the trait.
///
/// What's LEFT is the actual public contract surface: fns, types,
/// traits. Those are what the rule cares about.
const ITEM_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "type_item",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut seen_private = false;
        let mut diagnostics = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if !ITEM_KINDS.contains(&child.kind()) {
                continue;
            }
            let is_public = has_pub_visibility(child, source_bytes);
            if is_public && seen_private {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "exports-at-top".into(),
                    message: "Public item declared after a private item — \
                              move all `pub` items above the private \
                              helpers so the module's API is visible at a glance."
                        .into(),
                    severity: Severity::Warning,
                });
            }
            if !is_public {
                seen_private = true;
            }
        }
        diagnostics
    }
}

fn has_pub_visibility(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && child.utf8_text(source).is_ok_and(|t| t.starts_with("pub"))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn allows_public_then_private() {
        let source = "pub fn a() {}\npub fn b() {}\nfn helper() {}\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_public_after_private() {
        let source = "fn helper() {}\npub fn exposed() {}\n";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_all_public() {
        assert!(run_on("pub fn a() {}\npub fn b() {}\n").is_empty());
    }

    #[test]
    fn allows_all_private() {
        assert!(run_on("fn a() {}\nfn b() {}\n").is_empty());
    }

    #[test]
    fn allows_private_mod_above_pub_use() {
        // Canonical Rust module layout: private `mod` declarations at the
        // top, then `pub use` re-exports. This is the idiom the rule
        // previously fought with.
        let source = "mod internal;\nmod schema;\npub use schema::Config;\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_mod_pub_use_then_pub_fn_then_private_fn() {
        let source = "mod a;\npub use a::X;\npub fn exposed() {}\nfn helper() {}\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_helper_before_pub_fn() {
        let source = "mod a;\nfn helper() {}\npub fn exposed() {}\n";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_private_const_above_pub_struct() {
        // Private module-level constants are data, not "helpers" — they
        // belong near the imports.
        let source = "const MARKER: &str = \"TODO:\";\npub struct Parse { text: String }\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_private_static_above_pub_fn() {
        let source = "static BUFFER: &[u8] = b\"\";\npub fn parse() {}\n";
        assert!(run_on(source).is_empty());
    }
}
