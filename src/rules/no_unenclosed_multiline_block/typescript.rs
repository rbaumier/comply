//! no-unenclosed-multiline-block backend — flag braceless if/for/while with body on next line.

use crate::diagnostic::{Diagnostic, Severity};

/// Return the keyword label for the diagnostic message.
fn keyword_label(kind: &str) -> &'static str {
    match kind {
        "if_statement" => "if",
        "for_statement" | "for_in_statement" => "for",
        "while_statement" => "while",
        _ => "if",
    }
}

crate::ast_check! { on ["if_statement", "for_statement", "for_in_statement", "while_statement"] => |node, source, ctx, diagnostics|
    let kind = node.kind();

    // For if_statement: check "consequence" field. For for/while: check "body" field.
    let body_field = match kind {
        "if_statement" => "consequence",
        _ => "body",
    };

    let Some(body) = node.child_by_field_name(body_field) else { return };

    // If the body is a statement_block (curly braces), it's fine.
    if body.kind() == "statement_block" {
        return;
    }

    // The body is not enclosed in braces. Check if it's on a different line.
    let stmt_line = node.start_position().row;
    let body_line = body.start_position().row;

    if body_line > stmt_line {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-unenclosed-multiline-block".into(),
            message: format!(
                "`{}` body is on the next line without curly braces \u{2014} wrap it in `{{}}`.",
                keyword_label(kind),
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_multiline_if_without_braces() {
        let d = crate::rules::test_helpers::run_ts("if (condition)\n    doSomething();", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unenclosed-multiline-block");
    }

    #[test]
    fn flags_multiline_for_without_braces() {
        let d =
            crate::rules::test_helpers::run_ts("for (const x of items)\n    process(x);", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_braced_if() {
        let d =
            crate::rules::test_helpers::run_ts("if (condition) {\n    doSomething();\n}", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_single_line_if() {
        let d = crate::rules::test_helpers::run_ts("if (condition) doSomething();", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_while_without_braces() {
        let d = crate::rules::test_helpers::run_ts("while (running)\n    tick();", &Check);
        assert_eq!(d.len(), 1);
    }
}
