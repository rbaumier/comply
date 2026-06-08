//! prefer-code-point backend — flag `charCodeAt` and `String.fromCharCode`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["charCodeAt", "fromCharCode"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    match prop_name {
        "charCodeAt" => {
            let pos = prop.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-code-point".into(),
                message: "Prefer `String#codePointAt()` over `String#charCodeAt()`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        "fromCharCode" => {
            // Verify object is `String`
            let Some(obj) = func.child_by_field_name("object") else { return };
            if obj.utf8_text(source).unwrap_or("") != "String" {
                return;
            }
            let pos = prop.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-code-point".into(),
                message: "Prefer `String.fromCodePoint()` over `String.fromCharCode()`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
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
    fn flags_char_code_at() {
        let d = run_on("const c = str.charCodeAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("codePointAt"));
    }

    #[test]
    fn flags_from_char_code() {
        let d = run_on("const s = String.fromCharCode(65);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fromCodePoint"));
    }

    #[test]
    fn allows_code_point_at() {
        assert!(run_on("const c = str.codePointAt(0);").is_empty());
    }

    #[test]
    fn allows_from_code_point() {
        assert!(run_on("const s = String.fromCodePoint(65);").is_empty());
    }
}
