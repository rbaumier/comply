//! prefer-dom-node-text-content backend — flag `.innerText` usage.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] prefilter = ["innerText"] => |node, source, ctx, diagnostics|
    let Some(prop) = node.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "innerText" {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-dom-node-text-content".into(),
        message: "Prefer `.textContent` over `.innerText`.".into(),
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
    fn flags_inner_text_read() {
        let d = run_on("const t = el.innerText;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("textContent"));
    }

    #[test]
    fn flags_inner_text_assign() {
        let d = run_on(r#"el.innerText = "hello";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_text_content() {
        assert!(run_on("const t = el.textContent;").is_empty());
    }
}
