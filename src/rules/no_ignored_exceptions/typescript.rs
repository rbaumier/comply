//! no-ignored-exceptions backend — flag empty `catch` blocks.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "catch_clause" {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };

    // Check if the body is empty (only whitespace/comments).
    let named_count = body.named_child_count();
    let mut has_real_statement = false;

    for i in 0..named_count {
        let Some(child) = body.named_child(i) else { continue };
        match child.kind() {
            "comment" | "empty_statement" => continue,
            _ => {
                has_real_statement = true;
                break;
            }
        }
    }

    if has_real_statement {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-exceptions".into(),
        message: "Empty `catch` block silently swallows the exception — log or re-throw it.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_catch() {
        let src = "try { doSomething(); } catch (e) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_catch_with_only_comments() {
        let src = r#"
try {
  doSomething();
} catch (e) {
  // intentionally empty
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_catch_with_handler() {
        let src = "try { doSomething(); } catch (e) { console.error(e); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_catch_with_rethrow() {
        let src = "try { doSomething(); } catch (e) { throw e; }";
        assert!(run_on(src).is_empty());
    }
}
