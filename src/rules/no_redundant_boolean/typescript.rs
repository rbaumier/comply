//! no-redundant-boolean backend — redundant boolean literal in return or
//! condition.
//!
//! Detects three AST patterns:
//!
//! 1. Ternary with boolean literal branches — `cond ? true : false`.
//! 2. Strict comparison against a boolean literal — `x === true`, `x !== false`.
//! 3. `if (cond) return true/false; [else] return opposite;` — whether the
//!    else is explicit, an implicit trailing return, or a block body.
//!
//! Pure node-kind checks so string occurrences in comments or literals are
//! never matched.

use crate::diagnostic::{Diagnostic, Severity};

fn is_bool_literal(node: tree_sitter::Node) -> bool {
    matches!(node.kind(), "true" | "false")
}

/// Return the single expression returned by a `return_statement`, if any.
fn return_value(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() != "return_statement" {
        return None;
    }
    // tree-sitter TS represents `return x;` as return_statement -> expression.
    // The returned expression is the first named child.
    (0..node.named_child_count()).find_map(|i| node.named_child(i))
}

/// If `body` is a statement_block containing exactly one statement, return
/// that statement. Otherwise return `body` itself — supports both
/// `if (x) return true;` and `if (x) { return true; }`.
fn unwrap_single_stmt(body: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if body.kind() == "statement_block" {
        let mut stmts = Vec::new();
        for i in 0..body.named_child_count() {
            if let Some(c) = body.named_child(i) {
                stmts.push(c);
            }
        }
        if stmts.len() == 1 {
            return Some(stmts[0]);
        }
        return None;
    }
    Some(body)
}

/// `stmt` returns a boolean literal → return that literal's kind ("true" / "false").
fn returns_bool_literal(stmt: tree_sitter::Node) -> Option<&'static str> {
    let inner = unwrap_single_stmt(stmt)?;
    let value = return_value(inner)?;
    match value.kind() {
        "true" => Some("true"),
        "false" => Some("false"),
        _ => None,
    }
}

fn push_diag(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &crate::rules::backend::CheckCtx,
    node: tree_sitter::Node,
    message: &str,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-redundant-boolean".into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

crate::ast_check! { on ["ternary_expression", "binary_expression", "if_statement"] => |node, source, ctx, diagnostics|
    let _ = source;
match node.kind() {
        // Pattern 1: ternary with boolean literal branches.
        "ternary_expression" => {
            let consequence = node.child_by_field_name("consequence");
            let alternative = node.child_by_field_name("alternative");
            if let (Some(c), Some(a)) = (consequence, alternative)
                && is_bool_literal(c)
                && is_bool_literal(a)
            {
                push_diag(
                    diagnostics,
                    ctx,
                    node,
                    "Redundant ternary — simplify to the condition itself (or its negation).",
                );
            }
        }

        // Pattern 2: strict comparison against a boolean literal.
        "binary_expression" => {
            let Some(op) = node.child_by_field_name("operator") else { return };
            let op_text = op.utf8_text(source).unwrap_or("");
            if op_text != "===" && op_text != "!==" {
                return;
            }
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            let compares_bool = left.is_some_and(is_bool_literal)
                || right.is_some_and(is_bool_literal);
            if compares_bool {
                push_diag(
                    diagnostics,
                    ctx,
                    node,
                    "Redundant boolean comparison — use the value directly.",
                );
            }
        }

        // Pattern 3: if/else (or if + trailing return) returning boolean literals.
        "if_statement" => {
            let Some(consequence) = node.child_by_field_name("consequence") else { return };
            let Some(cons_bool) = returns_bool_literal(consequence) else { return };

            // 3a. Explicit else branch: `if (c) return X; else return Y;`
            if let Some(alt) = node.child_by_field_name("alternative") {
                // tree-sitter TS wraps this in an `else_clause`; unwrap its child.
                let alt_body = if alt.kind() == "else_clause" {
                    alt.named_child(0).unwrap_or(alt)
                } else {
                    alt
                };
                if let Some(alt_bool) = returns_bool_literal(alt_body)
                    && cons_bool != alt_bool
                {
                    push_diag(
                        diagnostics,
                        ctx,
                        node,
                        "Redundant if/else returning boolean literals — return the condition directly.",
                    );
                }
                return;
            }

            // 3b. No else — look at the next sibling statement.
            let Some(next) = node.next_named_sibling() else { return };
            if let Some(next_bool) = returns_bool_literal(next)
                && cons_bool != next_bool
            {
                push_diag(
                    diagnostics,
                    ctx,
                    node,
                    "Redundant if/else returning boolean literals — return the condition directly.",
                );
            }
        }

        _ => {}
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_ternary_true_false() {
        assert_eq!(run_on("const x = cond ? true : false;").len(), 1);
    }

    #[test]
    fn flags_ternary_false_true() {
        assert_eq!(run_on("const x = cond ? false : true;").len(), 1);
    }

    #[test]
    fn flags_strict_equals_true() {
        assert_eq!(run_on("if (x === true) doSomething();").len(), 1);
    }

    #[test]
    fn flags_strict_not_equals_false() {
        assert_eq!(run_on("if (x !== false) doSomething();").len(), 1);
    }

    #[test]
    fn flags_if_return_true_else_return_false() {
        assert_eq!(run_on("if (isValid) return true;\nreturn false;").len(), 1);
    }

    #[test]
    fn flags_if_else_block() {
        let src =
            "function f() {\n  if (c) {\n    return true;\n  } else {\n    return false;\n  }\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_normal_ternary() {
        assert!(run_on("const x = cond ? 'a' : 'b';").is_empty());
    }

    #[test]
    fn allows_comment_mentioning_true() {
        assert!(run_on("// returns true if valid").is_empty());
    }

    // Regression: hybrid text+AST impl flagged boolean-literal substrings
    // inside string contents. Pure node-kind checks avoid this.
    #[test]
    fn allows_boolean_literal_inside_string() {
        assert!(run_on(r#"const s = "x === true"; "#).is_empty());
    }

    #[test]
    fn allows_boolean_literal_inside_template_string() {
        assert!(run_on("const s = `cond ? true : false`;").is_empty());
    }

    #[test]
    fn allows_if_else_same_bool() {
        // Not a redundant-boolean case: both branches return the same literal.
        assert!(run_on("if (c) return true;\nreturn true;").is_empty());
    }

    #[test]
    fn allows_strict_equals_non_bool() {
        assert!(run_on("if (x === 1) f();").is_empty());
    }
}
