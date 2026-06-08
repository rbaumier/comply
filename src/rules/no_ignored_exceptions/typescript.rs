//! no-ignored-exceptions backend — flag empty `catch` blocks.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["catch_clause"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };

    // A real statement (anything but a comment / bare `;`) means the catch
    // handles the error — not our concern.
    let named_count = body.named_child_count();
    for i in 0..named_count {
        let Some(child) = body.named_child(i) else { continue };
        if !matches!(child.kind(), "comment" | "empty_statement") {
            return;
        }
    }

    // Otherwise-empty catch: a comment documents intentional suppression (the
    // ESLint `no-empty` convention) — e.g. parse-and-fall-back. Only a bare
    // empty block silently swallows the exception.
    if body
        .utf8_text(source)
        .is_ok_and(|t| t.contains("//") || t.contains("/*"))
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-ignored-exceptions".into(),
        message: "Empty `catch` block silently swallows the exception — log or re-throw it.".into(),
        severity: Severity::Error,
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
    fn flags_empty_catch() {
        let src = "try { doSomething(); } catch (e) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for #267: a comment documents intentional suppression — the
    // ESLint `no-empty` convention — so a comment-only catch is allowed.
    #[test]
    fn allows_catch_with_only_comments() {
        let src = r#"
try {
  doSomething();
} catch (e) {
  // intentionally empty
}
"#;
        assert!(run_on(src).is_empty());
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
