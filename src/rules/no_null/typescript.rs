use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["null"] => |node, source, ctx, diagnostics|
    // Skip if inside a comment (parent is a comment node).
    if let Some(parent) = node.parent()
        && parent.kind() == "comment" {
            return;
        }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-null".into(),
        message: "Use `undefined` instead of `null`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_null_literal() {
        assert_eq!(run("const x = null;").len(), 1);
    }

    #[test]
    fn flags_null_comparison() {
        assert_eq!(run("if (x === null) {}").len(), 1);
    }

    #[test]
    fn flags_return_null() {
        assert_eq!(run("return null;").len(), 1);
    }

    #[test]
    fn allows_undefined() {
        assert!(run("const x = undefined;").is_empty());
    }
}
