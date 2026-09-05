//! no-gratuitous-expression Rust backend.
//!
//! Flag the two boolean shapes whose value is fixed by a literal operand: a
//! literal `if true` / `if false` condition, and a `&& false` / `|| true`
//! short-circuit.
//!
//! Neither shape is flagged when its enclosing statement carries
//! `#[allow(clippy::overly_complex_bool_expr)]` / `#[allow(clippy::nonminimal_bool)]`
//! (the overlapping clippy lints — the author opted out). A short-circuit is
//! also spared when the operand adjacent to the literal is a `cfg!(...)` macro
//! (a compile-time debug toggle, not a gratuitous constant), and `if false` is
//! spared when it guards a non-empty body with no `else`: that body is
//! type-checked on every build and never run, so removing it is a semantic edit
//! rather than the syntactic reduction the remediation assumes.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// The constant a literal operand forces on a `&&`/`||` expression.
#[derive(Clone, Copy)]
enum ShortCircuit {
    AlwaysFalse,
    AlwaysTrue,
}

impl ShortCircuit {
    fn message(self) -> &'static str {
        match self {
            Self::AlwaysFalse => {
                "Gratuitous expression: expression is always false (short-circuited by `&& false`)."
            }
            Self::AlwaysTrue => {
                "Gratuitous expression: expression is always true (short-circuited by `|| true`)."
            }
        }
    }
}

crate::ast_check! { on ["if_expression", "binary_expression"] => |node, source, ctx, diagnostics|
{
    let message = match node.kind() {
        "if_expression" => {
            let Some(condition) = node.child_by_field_name("condition") else { return };
            if condition.kind() != "boolean_literal" {
                return;
            }
            let Ok(literal) = condition.utf8_text(source) else { return };
            match literal {
                "true" => "Gratuitous expression: condition is always true.",
                "false" if !is_compile_only_block(node) => {
                    "Gratuitous expression: condition is always false."
                }
                _ => return,
            }
        }
        "binary_expression" => {
            // Only a literal operand of `&&`/`||` is gratuitous. `x == x` /
            // `x != x` is NOT flagged: it is the IEEE 754 NaN-detection idiom
            // (`x != x` is true iff `x` is NaN, the only value not equal to
            // itself). Without type inference the operand cannot be proven to
            // be a float, so this self-comparison form is left to
            // `no-identical-expressions` (which also exempts it). See #5788.
            let Some(short_circuit) = short_circuit_shape(node, source) else { return };
            if operand_adjacent_to_literal_is_cfg(node, source) {
                return;
            }
            short_circuit.message()
        }
        _ => return,
    };
    // Both shapes overlap clippy's `overly_complex_bool_expr` /
    // `nonminimal_bool`. An author who annotates the enclosing statement with
    // either lint (as `allow` or `expect`) has explicitly opted out — defer to
    // it, as for clippy `#[allow]` in other rules. This is the canonical
    // manually-toggle-able debug block (flip `false` -> `true`), not a refactor
    // leftover.
    if crate::rules::rust_helpers::has_clippy_allow(node, source, "overly_complex_bool_expr")
        || crate::rules::rust_helpers::has_clippy_allow(node, source, "nonminimal_bool")
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-gratuitous-expression".into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}
}

/// The constant forced on a `binary_expression` by a boolean literal operand of
/// a short-circuit operator: `x && false` / `false && x` is always false,
/// `x || true` / `true || x` is always true. Any other operator, or a `&&`/`||`
/// with no literal operand, yields `None`.
///
/// Reading the `operator` and `left`/`right` fields — rather than the node's
/// text — is what keeps a closure out: `unwrap_or_else(|| true)` puts its `||`
/// inside a `closure_expression` nested in an operand, never on the binary
/// node's own operator or operand.
fn short_circuit_shape(node: Node, source: &[u8]) -> Option<ShortCircuit> {
    let operator = node.child_by_field_name("operator")?;
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    let has_operand = |literal: &str| {
        [left, right].into_iter().any(|operand| {
            operand.kind() == "boolean_literal"
                && matches!(operand.utf8_text(source), Ok(text) if text == literal)
        })
    };
    match operator.kind() {
        "&&" if has_operand("false") => Some(ShortCircuit::AlwaysFalse),
        "||" if has_operand("true") => Some(ShortCircuit::AlwaysTrue),
        _ => None,
    }
}

/// True if the `if_expression` is a compile-only block: a non-empty body and no
/// `else`. Its statements are type-checked on every build and never executed —
/// the shortest way in Rust to say "compile this, do not run it", used to keep
/// hand-written catalogues in sync with the types they enumerate. Deleting it
/// changes what the compiler sees, unlike an empty body or an `if false { .. }
/// else { .. }` whose `else` branch is the live one.
fn is_compile_only_block(node: Node) -> bool {
    if node.child_by_field_name("alternative").is_some() {
        return false;
    }
    let Some(body) = node.child_by_field_name("consequence") else {
        return false;
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .any(|child| !crate::rules::rust_helpers::is_comment_node(child))
}

/// True if the operand on the opposite side of the literal `false`/`true` in a
/// `binary_expression` is a `cfg!(...)` macro invocation, i.e. a compile-time
/// toggle (`if cfg!(debug_assertions) && false { ... }`). Such an expression is
/// an intentional manual switch — the author flips the literal to re-enable a
/// gated path — not a gratuitous always-false/always-true constant.
fn operand_adjacent_to_literal_is_cfg(node: Node, source: &[u8]) -> bool {
    let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) else {
        return false;
    };
    // The literal sits on one side; the cfg! must be the other operand.
    let operand = if right.kind() == "boolean_literal" {
        left
    } else if left.kind() == "boolean_literal" {
        right
    } else {
        return false;
    };
    crate::rules::rust_helpers::is_cfg_macro_invocation(operand, source)
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
    fn flags_if_true() {
        let d = run_on("fn f() { if true { do_stuff(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn allows_normal_conditions() {
        assert!(run_on("fn f(x: i32) { if x > 0 { do_stuff(); } }").is_empty());
    }

    #[test]
    fn allows_cfg_toggle_with_overly_complex_bool_expr_allow() {
        // rust-analyzer crates/syntax/src/syntax_node.rs:52 — the canonical
        // manually-toggle-able debug block, annotated with the exact clippy
        // lint this rule overlaps.
        let source = "fn finish() {\n\
                      #[allow(clippy::overly_complex_bool_expr)]\n\
                      if cfg!(debug_assertions) && false { let _ = 1; }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_nonminimal_bool_allow_variant() {
        let source = "fn f() {\n\
                      #[allow(clippy::nonminimal_bool)]\n\
                      if g() && false { do_stuff(); }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_expect_overly_complex_bool_expr() {
        let source = "fn f() {\n\
                      #[expect(clippy::overly_complex_bool_expr)]\n\
                      if g() || true { do_stuff(); }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_cfg_toggle_without_allow() {
        // The `cfg!(...)` operand alone marks an intentional compile-time toggle.
        assert!(run_on("fn f() { if cfg!(debug_assertions) && false { } }").is_empty());
    }

    #[test]
    fn allows_qualified_cfg_toggle() {
        assert!(run_on("fn f() { if core::cfg!(debug_assertions) && false { } }").is_empty());
    }

    #[test]
    fn flags_bare_and_false_without_allow() {
        let d = run_on("fn f(x: bool) { let _ = x && false; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn flags_bare_or_true_without_allow() {
        let d = run_on("fn f(y: bool) { if y || true { do_stuff(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always true"));
    }

    #[test]
    fn still_flags_and_false_with_unrelated_allow() {
        // An unrelated `#[allow(dead_code)]` must not suppress.
        let d = run_on("fn f(x: bool) {\n#[allow(dead_code)]\nlet _ = x && false;\n}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_closure_body_true_inside_and_chain() {
        // apache/iceberg-rust crates/iceberg/src/delete_file_index.rs:190 — the
        // `|| true` is a zero-argument closure body, not a logical-or operand.
        let source = "fn f() {\n\
                      let _ = seq_num\n\
                          .map(|seq_num| entry.sequence_number() > Some(seq_num))\n\
                          .unwrap_or_else(|| true)\n\
                          && data_file.partition_spec_id == delete.partition_spec_id;\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_closure_body_true_inside_or_chain() {
        let source = "fn f() { let _ = opt.unwrap_or_else(|| true) || other(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_literal_false_as_left_operand() {
        let d = run_on("fn f(x: bool) { let _ = false && x; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn allows_compile_only_if_false_block() {
        // dtolnay/syn tests/test_expr.rs:929 — the body is type-checked on every
        // build and never run; deleting it drops that compile-time coverage.
        let source = "fn iter(f: &mut dyn FnMut(Expr)) {\n\
                      if false {\n\
                          f(Expr::Path(ExprPath { attrs: Vec::new() }));\n\
                      }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_empty_if_false_block() {
        // Nothing is compiled inside, so removing it is a syntactic reduction.
        let d = run_on("fn f() { if false { } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn flags_if_false_with_else() {
        // The `else` branch is the live one; the `if` is noise.
        let d = run_on("fn f() { if false { a(); } else { b(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
    }

    #[test]
    fn allows_empty_if_false_with_clippy_allow() {
        let source = "fn f() {\n\
                      #[allow(clippy::nonminimal_bool)]\n\
                      if false { }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_true_with_clippy_allow() {
        let source = "fn f() {\n\
                      #[expect(clippy::overly_complex_bool_expr)]\n\
                      if true { do_stuff(); }\n}";
        assert!(run_on(source).is_empty());
    }

    // `x != x` / `x == x` is the IEEE 754 NaN-detection idiom, not a gratuitous
    // always-false/always-true comparison. See issue #5788.
    #[test]
    fn allows_self_inequality_nan_idiom() {
        assert!(run_on("fn is_nan(self) -> bool { self != self }").is_empty());
    }

    #[test]
    fn allows_self_equality_nan_idiom() {
        assert!(run_on("fn not_nan(x: f64) -> bool { x == x }").is_empty());
    }
}
