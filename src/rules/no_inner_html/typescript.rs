//! no-inner-html backend — flag `.innerHTML = ...` / `.outerHTML = ...`
//! assignments (regular and augmented like `+=`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression", "augmented_assignment_expression"] prefilter = ["innerHTML", "outerHTML"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }
    let Some(prop) = left.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if prop_text != "innerHTML" && prop_text != "outerHTML" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-inner-html".into(),
        message: format!("Writing to `.{prop_text}` is an XSS sink — use `textContent` or sanitize via DOMPurify."),
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
    fn flags_inner_html_assignment() {
        assert_eq!(run_on("el.innerHTML = raw;").len(), 1);
    }

    #[test]
    fn flags_outer_html_assignment() {
        assert_eq!(run_on("el.outerHTML = raw;").len(), 1);
    }

    #[test]
    fn flags_inner_html_plus_equals() {
        assert_eq!(run_on("el.innerHTML += raw;").len(), 1);
    }

    #[test]
    fn allows_text_content_assignment() {
        assert!(run_on("el.textContent = raw;").is_empty());
    }

    #[test]
    fn allows_reading_inner_html() {
        assert!(run_on("const s = el.innerHTML;").is_empty());
    }
}
