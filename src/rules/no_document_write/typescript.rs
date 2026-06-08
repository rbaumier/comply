//! no-document-write backend — flag `document.write` / `document.writeln` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["document"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "document.write" && name != "document.writeln" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-document-write".into(),
        message: format!("`{name}()` is an XSS vector and re-opens the document — use DOM APIs (`appendChild`, sanitized `innerHTML`) instead."),
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
    fn flags_document_write() {
        assert_eq!(run_on(r#"document.write("<p>hi</p>");"#).len(), 1);
    }

    #[test]
    fn flags_document_writeln() {
        assert_eq!(run_on(r#"document.writeln("hi");"#).len(), 1);
    }

    #[test]
    fn allows_other_document_method() {
        assert!(run_on("document.createElement('div');").is_empty());
    }
}
