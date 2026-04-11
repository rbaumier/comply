//! prefer-logical-operator-over-ternary — flag ternaries replaceable by `||`/`??`.
//!
//! Patterns:
//! - `foo ? foo : bar`  ->  `foo || bar`  (or `foo ?? bar`)
//! - `!bar ? foo : bar` ->  `bar || foo`  (test.argument === alternate)

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "ternary_expression" {
        return;
    }

    let test = match node.child_by_field_name("condition") {
        Some(c) => c,
        None => return,
    };
    let consequent = match node.child_by_field_name("consequence") {
        Some(c) => c,
        None => return,
    };
    let alternate = match node.child_by_field_name("alternative") {
        Some(a) => a,
        None => return,
    };

    let test_text = test.utf8_text(source).unwrap_or("");
    let consequent_text = consequent.utf8_text(source).unwrap_or("");
    let alternate_text = alternate.utf8_text(source).unwrap_or("");

    // Pattern 1: `foo ? foo : bar` — test === consequent
    if same_text(test_text, consequent_text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-logical-operator-over-ternary".into(),
            message: format!(
                "Prefer `{test_text} || {alternate_text}` (or `??`) over `{test_text} ? {test_text} : {alternate_text}`."
            ),
            severity: Severity::Warning,
        });
        return;
    }

    // Pattern 2: `!bar ? foo : bar` — negated test.argument === alternate
    if test.kind() == "unary_expression" {
        let op = test
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok())
            .unwrap_or("");
        if op == "!"
            && let Some(arg) = test.child_by_field_name("argument") {
                let arg_text = arg.utf8_text(source).unwrap_or("");
                if same_text(arg_text, alternate_text) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-logical-operator-over-ternary".into(),
                        message: format!(
                            "Prefer `{alternate_text} || {consequent_text}` (or `??`) over \
                             `!{alternate_text} ? {consequent_text} : {alternate_text}`."
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
    }
}

/// Compare two source text snippets after trimming whitespace.
fn same_text(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    !a.is_empty() && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_test_equals_consequent() {
        let d = run_on("const x = foo ? foo : bar;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("||"));
    }

    #[test]
    fn flags_negated_test_equals_alternate() {
        let d = run_on("const x = !bar ? foo : bar;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("||"));
    }

    #[test]
    fn allows_distinct_arms() {
        assert!(run_on("const x = a ? b : c;").is_empty());
    }

    #[test]
    fn allows_test_equals_alternate_no_negation() {
        // `foo ? bar : foo` — not a simple || pattern
        assert!(run_on("const x = foo ? bar : foo;").is_empty());
    }

    #[test]
    fn flags_member_expression() {
        let d = run_on("const x = a.b ? a.b : c;");
        assert_eq!(d.len(), 1);
    }
}
