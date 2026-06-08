use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let body_text =
            &ctx.source[try_stmt.block.span.start as usize..try_stmt.block.span.end as usize];

        if !body_text.contains("new URL(") {
            return;
        }

        let Some(handler) = &try_stmt.handler else {
            return;
        };

        let catch_text =
            &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];

        let is_validation_pattern = body_text.contains("return true")
            || body_text.contains("return new URL")
            || catch_text.contains("return false")
            || catch_text.contains("return null")
            || catch_text.contains("return undefined");

        if !is_validation_pattern {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, try_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `URL.canParse(url)` instead of try-catch with `new URL()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(code, &Check)
    }


    #[test]
    fn flags_try_catch_url_validation() {
        let code = r#"
            function isValidUrl(url) {
                try { new URL(url); return true; }
                catch { return false; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }


    #[test]
    fn flags_try_catch_return_null() {
        let code = r#"
            function parseUrl(url) {
                try { return new URL(url); }
                catch { return null; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }


    #[test]
    fn allows_url_canparse() {
        assert!(run("const valid = URL.canParse(url);").is_empty());
    }


    #[test]
    fn allows_try_catch_without_validation_return() {
        let code = r#"
            try { const u = new URL(url); process(u); }
            catch (e) { console.error(e); }
        "#;
        assert!(run(code).is_empty());
    }
}
