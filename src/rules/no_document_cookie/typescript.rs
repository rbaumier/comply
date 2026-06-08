//! no-document-cookie — flag direct `document.cookie` access.
//!
//! Matches `member_expression` nodes where the object is `document`
//! and the property is `cookie`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] prefilter = ["document"] => |node, source, ctx, diagnostics|
    let Some(obj) = node.child_by_field_name("object") else { return };
    let Some(prop) = node.child_by_field_name("property") else { return };

    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };

    if obj_text != "document" || prop_text != "cookie" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-document-cookie".into(),
        message: "Do not use `document.cookie` directly — use a cookie library instead.".into(),
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
    fn flags_cookie_read() {
        assert_eq!(run_on("const c = document.cookie;").len(), 1);
    }

    #[test]
    fn flags_cookie_write() {
        assert_eq!(run_on(r#"document.cookie = "a=1";"#).len(), 1);
    }

    #[test]
    fn allows_unrelated_member() {
        assert!(run_on("const t = document.title;").is_empty());
    }
}
