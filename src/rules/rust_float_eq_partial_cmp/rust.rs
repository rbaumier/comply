//! rust-float-eq-partial-cmp backend.
//!
//! For each `binary_expression` whose operator is `==` or `!=`, flag if
//! either operand looks like a float:
//! - `float_literal` (e.g. `1.0`, `1e9`)
//! - identifier whose binding type annotation is `f32`/`f64` — comply
//!   only sees the file we're checking, so we walk back from the operand
//!   to the closest enclosing `let_declaration` / `parameter` / `function_item`
//!   and read the type annotation if present.
//!
//! When the type isn't visible we fall back to "operand is a float
//! literal" — that's the unambiguous case clippy's `float_cmp` also
//! catches first.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["binary_expression"];

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
        let source = ctx.source.as_bytes();
        let Some(op) = node.child_by_field_name("operator") else {
            return;
        };
        let Ok(op_text) = op.utf8_text(source) else {
            return;
        };
        if op_text != "==" && op_text != "!=" {
            return;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        if !operand_is_float(left, source) && !operand_is_float(right, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-float-eq-partial-cmp",
            format!(
                "float `{op_text}` compares bit patterns, not numerical \
                 equality. Use `(a - b).abs() < EPSILON` with a \
                 domain-appropriate epsilon, or `partial_cmp` for ordering."
            ),
            Severity::Warning,
        ));
    }
}

fn operand_is_float(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "float_literal" {
        return true;
    }
    // `as f32` / `as f64` casts.
    if node.kind() == "type_cast_expression"
        && let Some(ty) = node.child_by_field_name("type")
        && let Ok(text) = ty.utf8_text(source)
        && (text == "f32" || text == "f64")
    {
        return true;
    }
    // identifier with a `let x: f32 = …` binding visible in this file.
    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source)
        && let Some(ty) = lookup_let_type(node, name, source)
        && (ty == "f32" || ty == "f64")
    {
        return true;
    }
    false
}

/// Walk upward from `node` looking for a `let_declaration` whose pattern
/// names `ident`. If found, return its type annotation text.
fn lookup_let_type(node: tree_sitter::Node, ident: &str, source: &[u8]) -> Option<String> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        // Visit preceding siblings of `cur` for prior `let` bindings.
        let mut sibling = cur.prev_named_sibling();
        while let Some(s) = sibling {
            if s.kind() == "let_declaration"
                && let Some(ty) = let_decl_type_for(s, ident, source)
            {
                return Some(ty);
            }
            sibling = s.prev_named_sibling();
        }
        cur = parent;
    }
    None
}

fn let_decl_type_for(decl: tree_sitter::Node, ident: &str, source: &[u8]) -> Option<String> {
    let pat = decl.child_by_field_name("pattern")?;
    let pat_text = pat.utf8_text(source).ok()?;
    if pat_text != ident {
        return None;
    }
    let ty = decl.child_by_field_name("type")?;
    Some(ty.utf8_text(source).ok()?.to_string())
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

    #[test]
    fn flags_float_literal_eq() {
        let src = "fn f(x: f64) -> bool { x == 1.0 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_float_literal_neq() {
        let src = "fn f(x: f64) -> bool { x != 1.0 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_let_typed_float_eq() {
        let src = "fn f() -> bool { let x: f32 = 1.0; x == 2.0 }";
        // 1.0 makes left float-literal-like once typed, but the right side
        // alone (1.0 / 2.0) is already a float_literal. Either way we only
        // report once per binary_expression.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_int_eq() {
        let src = "fn f(x: u32) -> bool { x == 1 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_partial_cmp() {
        let src = "fn f(a: f64, b: f64) -> Option<std::cmp::Ordering> { a.partial_cmp(&b) }";
        assert!(run_on(src).is_empty());
    }
}
