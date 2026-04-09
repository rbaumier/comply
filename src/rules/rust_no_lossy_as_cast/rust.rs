//! rust-no-lossy-as-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax)
//! and flags casts where the destination type is in our "narrowing
//! or precision-losing" set:
//!
//! - integer narrowing (`u32 as u8`, `i64 as i32`, etc.)
//! - float to integer (`f64 as u32`)
//! - signed/unsigned reinterpretation can wrap, but we leave it for
//!   `clippy::cast_sign_loss` since the rule is more nuanced
//!
//! False positives exist (`SAFETY_CONSTANT as u8` where the value
//! is known small at compile time) — suppress with `// comply-ignore`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const NARROWING_TARGETS: &[&str] = &[
    "u8", "u16", "u32", "i8", "i16", "i32", "f32",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "type_cast_expression" {
                return;
            }
            let Some(type_node) = node.child_by_field_name("type") else {
                return;
            };
            let Ok(target) = type_node.utf8_text(source_bytes) else {
                return;
            };
            let target = target.trim();
            if !NARROWING_TARGETS.contains(&target) {
                return;
            }
            // Avoid false positive on widening when the source is also small.
            // We can't easily infer source types from a single AST node, so
            // accept the false positive — `try_into()` / `From::from()` are
            // both better than `as` even for "safe" casts.
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-lossy-as-cast".into(),
                message: format!(
                    "`as {target}` truncates / loses precision silently \
                     on overflow. Use `try_into()` (returns Result) for \
                     fallible narrowing, or `From::from(x)` if the cast \
                     is provably total."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_u32_to_u8() {
        assert_eq!(run_on("fn f(x: u32) -> u8 { x as u8 }").len(), 1);
    }

    #[test]
    fn flags_f64_to_u32() {
        assert_eq!(run_on("fn f(x: f64) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn allows_widening_to_u64() {
        assert!(run_on("fn f(x: u32) -> u64 { x as u64 }").is_empty());
    }

    #[test]
    fn allows_widening_to_i64() {
        assert!(run_on("fn f(x: i32) -> i64 { x as i64 }").is_empty());
    }
}
