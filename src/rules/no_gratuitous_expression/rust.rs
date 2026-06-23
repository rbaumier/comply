//! no-gratuitous-expression Rust backend.
//!
//! Flag boolean expressions that are always true or always false.
//!
//! A `&& false` / `|| true` short-circuit is NOT flagged when its enclosing
//! statement carries `#[allow(clippy::overly_complex_bool_expr)]` /
//! `#[allow(clippy::nonminimal_bool)]` (the overlapping clippy lints — the
//! author opted out), or when the operand adjacent to the literal is a
//! `cfg!(...)` macro (a compile-time debug toggle, not a gratuitous constant).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression", "binary_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "if_expression" => {
            let Some(condition) = node.child_by_field_name("condition") else { return };
            let Ok(cond_text) = condition.utf8_text(source) else { return };
            let inner = cond_text.trim();
            if inner == "true" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always true.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            } else if inner == "false" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: condition is always false.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        "binary_expression" => {
            let Ok(text) = node.utf8_text(source) else { return };
            // `&& false` / `|| true` overlaps clippy's `overly_complex_bool_expr`
            // / `nonminimal_bool`. An author who annotates the enclosing
            // statement with `#[allow(clippy::overly_complex_bool_expr)]` (or
            // `nonminimal_bool`, or `expect`) has explicitly opted out — defer to
            // it, as for clippy `#[allow]` in other rules. This is the canonical
            // manually-toggle-able debug block (flip `false` -> `true`), not a
            // refactor leftover.
            let short_circuit = (text.ends_with("&& false")
                || text.contains("&& false)")
                || text.contains("&& false;"))
                || (text.ends_with("|| true")
                    || text.contains("|| true)")
                    || text.contains("|| true;"));
            if short_circuit
                && (crate::rules::rust_helpers::has_clippy_allow(
                    node,
                    source,
                    "overly_complex_bool_expr",
                ) || crate::rules::rust_helpers::has_clippy_allow(
                    node,
                    source,
                    "nonminimal_bool",
                ) || operand_adjacent_to_literal_is_cfg(node, source))
            {
                return;
            }
            // Check `&& false` / `|| true`
            if text.ends_with("&& false") || text.contains("&& false)") || text.contains("&& false;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always false (short-circuited by `&& false`).".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
            if text.ends_with("|| true") || text.contains("|| true)") || text.contains("|| true;") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-gratuitous-expression".into(),
                    message: "Gratuitous expression: expression is always true (short-circuited by `|| true`).".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
            // `x == x` / `x != x` is NOT flagged: it is the IEEE 754
            // NaN-detection idiom (`x != x` is true iff `x` is NaN, the only
            // value not equal to itself). Without type inference the operand
            // cannot be proven to be a float, so this self-comparison form is
            // left to `no-identical-expressions` (which also exempts it). See
            // issue #5788.
        }
        _ => {}
    }
}

/// True if the operand on the opposite side of the literal `false`/`true` in a
/// `binary_expression` is a `cfg!(...)` macro invocation, i.e. a compile-time
/// toggle (`if cfg!(debug_assertions) && false { ... }`). Such an expression is
/// an intentional manual switch — the author flips the literal to re-enable a
/// gated path — not a gratuitous always-false/always-true constant.
fn operand_adjacent_to_literal_is_cfg(node: tree_sitter::Node, source: &[u8]) -> bool {
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
    operand.kind() == "macro_invocation"
        && operand
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            == Some("cfg")
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
    fn flags_if_false() {
        let d = run_on("fn f() { if false { do_stuff(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("always false"));
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
