//! no-equals-in-for-termination backend — flag `==`/`===` in `for` loop
//! termination conditions.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the string contains `===` or `==` but not `!==` or `!=`.
fn contains_equality_op(s: &str) -> bool {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'=' {
            if i + 2 < bytes.len() && bytes[i + 1] == b'=' && bytes[i + 2] == b'=' {
                let not_negated = i == 0 || bytes[i - 1] != b'!';
                if not_negated {
                    return true;
                }
                i += 3;
                continue;
            }
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                let triple = i + 2 < bytes.len() && bytes[i + 2] == b'=';
                let not_negated = i == 0 || bytes[i - 1] != b'!';
                if !triple && not_negated {
                    return true;
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["for_statement"] => |node, source, ctx, diagnostics|
    // The condition is the second named child (after initializer).
    // In tree-sitter, for_statement has fields: initializer, condition, increment, body
    let Some(condition) = node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };
    if !contains_equality_op(cond_text) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-equals-in-for-termination".into(),
        message: "`for` loop uses equality (`==`/`===`) in termination — use `<`, `<=`, `>`, or `>=` instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_triple_equals() {
        assert_eq!(run_on("for (let i = 0; i === 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_double_equals() {
        assert_eq!(run_on("for (let i = 0; i == 10; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run_on("for (let i = 0; i !== 10; i++) {}").is_empty());
    }
}
