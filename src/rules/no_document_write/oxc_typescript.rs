use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_browser_injection_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["document"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "document" {
            return;
        }
        let method = member.property.name.as_str();
        if method != "write" && method != "writeln" {
            return;
        }
        // Calls inside a Playwright/Puppeteer browser-injection callback run in a
        // controlled automation browser, not the application DOM, so they are not
        // the XSS sink this rule targets.
        if is_inside_browser_injection_callback(node, semantic) {
            return;
        }
        let name = format!("document.{method}");
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-document-write".into(),
            message: format!("`{name}()` is an XSS vector and re-opens the document — use DOM APIs (`appendChild`, sanitized `innerHTML`) instead."),
            severity: Severity::Error,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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

    #[test]
    fn allows_document_write_in_evaluate_callback() {
        assert!(
            run_on(
                "context.evaluate(({ html, tag }) => {\n  document.open();\n  document.write(html);\n  document.close();\n}, { html, tag });"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_document_writeln_in_page_evaluate_callback() {
        assert!(run_on("page.evaluate(() => { document.writeln(markup); });").is_empty());
    }

    #[test]
    fn flags_top_level_document_write_outside_injection_callback() {
        assert_eq!(run_on("document.write(userInput);").len(), 1);
    }

    #[test]
    fn flags_document_write_in_non_injection_callback() {
        assert_eq!(run_on("setTimeout(() => { document.write(x); }, 0);").len(), 1);
    }
}
