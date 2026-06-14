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
        // Comparing against exact zero is legitimate: `0.0` is exactly
        // representable, so "is this exactly zero?" (e.g. `val.fract() == 0.0`
        // to test integer-ness, or an exact-divisibility `rem == 0.0`) is the
        // correct tool, not an epsilon. Clippy's `float_cmp` skips zero too.
        if operand_is_float_zero(left, source) || operand_is_float_zero(right, source) {
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

/// Is `node` a float-zero literal? Covers `0.0`, `0.0f64`, `0f64`, `0.`,
/// and a leading-minus `-0.0`. A negative zero appears as a `unary_expression`
/// (`-` applied to the literal) in the tree-sitter grammar, so unwrap it first.
fn operand_is_float_zero(node: tree_sitter::Node, source: &[u8]) -> bool {
    let lit = if node.kind() == "unary_expression" {
        match node.child_by_field_name("operator").and_then(|o| o.utf8_text(source).ok()) {
            Some("-") => match node.named_child(0) {
                Some(inner) => inner,
                None => return false,
            },
            _ => return false,
        }
    } else {
        node
    };
    if lit.kind() != "float_literal" {
        return false;
    }
    let Ok(text) = lit.utf8_text(source) else {
        return false;
    };
    // Strip an optional `f32`/`f64` type suffix, then check the mantissa is
    // numerically zero (`0`, `0.`, `0.0`, `0.000`, `0e0`).
    let mantissa = text.trim_end_matches("f64").trim_end_matches("f32");
    mantissa
        .parse::<f64>()
        .is_ok_and(|value| value == 0.0)
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

    #[test]
    fn allows_fract_eq_zero() {
        // tantivy columnar/src/value.rs: `fract == 0.0` (let-bound fract()).
        let src = "fn f(val: f64) -> bool { let fract = val.fract(); fract == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_remainder_eq_zero() {
        let src = "fn f(right_f: f64, right_as_i: i64) -> bool { \
                   let rem = right_f - (right_as_i as f64); rem == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_direct_fract_call_eq_zero() {
        let src = "fn f(x: f64) -> bool { x.fract() == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_neq() {
        let src = "fn f(x: f64) -> bool { x != 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_variants() {
        for src in [
            "fn f(x: f64) -> bool { x == 0.0f64 }",
            "fn f(x: f64) -> bool { x == 0f64 }",
            "fn f(x: f64) -> bool { x == 0. }",
            "fn f(x: f64) -> bool { x == -0.0 }",
            "fn f(x: f64) -> bool { 0.0 == x }",
        ] {
            assert!(run_on(src).is_empty(), "should not flag: {src}");
        }
    }

    #[test]
    fn flags_nonzero_float_literal_eq() {
        // Negative space: a genuine epsilon-needing comparison still fires.
        assert_eq!(run_on("fn f(x: f64) -> bool { x == 1.5 }").len(), 1);
    }

    #[test]
    fn flags_nonzero_sum_eq() {
        assert_eq!(run_on("fn f(a: f64, b: f64) -> bool { (a + b) == 0.3 }").len(), 1);
    }
}
