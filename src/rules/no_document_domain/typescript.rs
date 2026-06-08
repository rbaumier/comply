//! no-document-domain backend — flag `document.domain = ...` assignments.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] prefilter = ["document"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }
    let Some(obj) = left.child_by_field_name("object") else { return };
    let Some(prop) = left.child_by_field_name("property") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if obj_text != "document" || prop_text != "domain" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-document-domain".into(),
        message: "Assigning to `document.domain` weakens the same-origin policy — remove it and use `postMessage` or CORS instead.".into(),
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
    fn flags_document_domain_assignment() {
        assert_eq!(run_on(r#"document.domain = "example.com";"#).len(), 1);
    }

    #[test]
    fn allows_reading_document_domain() {
        assert!(run_on("const d = document.domain;").is_empty());
    }

    #[test]
    fn allows_unrelated_assignment() {
        assert!(run_on(r#"document.title = "hello";"#).is_empty());
    }
}
