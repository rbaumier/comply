//! rust-no-as-numeric-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax) and
//! flags any cast whose destination type is a numeric primitive. This
//! is deliberately stricter than `rust-no-lossy-as-cast`: even casts
//! that are trivially safe at the type level (e.g. `u8 as u64`) are
//! reported, because `From::from` documents the widening intent and
//! keeps future refactors honest.
//!
//! Tests are exempted — fuzz / numeric scaffolding inside `#[test]`
//! functions or `#[cfg(test)]` modules doesn't need this discipline.
//!
//! Non-numeric targets (pointer, reference, trait object) are ignored.
//! Casts like `*const u8 as usize` are false positives; suppress with
//! `// comply-ignore` on the offending line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const NUMERIC_TARGETS: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "usize", "isize", "f32",
    "f64",
];

const KINDS: &[&str] = &["type_cast_expression"];

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
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let Ok(target_raw) = type_node.utf8_text(source_bytes) else {
            return;
        };
        let target = target_raw.trim();
        if !NUMERIC_TARGETS.contains(&target) {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-as-numeric-cast".into(),
            message: format!(
                "`as {target}` masks overflow + precision semantics. Use \
                 `{target}::from(x)` for widening-safe casts or \
                 `{target}::try_from(x)?` for narrowing. Even on widening, \
                 `From` makes the conversion explicit and greppable."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_widening_u8_to_u64() {
        assert_eq!(run_on("fn f(x: u8) -> u64 { x as u64 }").len(), 1);
    }

    #[test]
    fn flags_narrowing_u64_to_u8() {
        assert_eq!(run_on("fn f(x: u64) -> u8 { x as u8 }").len(), 1);
    }

    #[test]
    fn flags_float_cast() {
        assert_eq!(run_on("fn f(x: i32) -> f64 { x as f64 }").len(), 1);
    }

    #[test]
    fn flags_isize_cast() {
        assert_eq!(run_on("fn f(p: *const u8) -> usize { p as usize }").len(), 1);
    }

    #[test]
    fn allows_non_numeric_target() {
        assert!(run_on("fn f(x: &str) -> &[u8] { x as &[u8] }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let source = "#[test]\nfn t() { let _ = 1u8 as u64; }";
        assert!(run_on(source).is_empty());
    }
}
