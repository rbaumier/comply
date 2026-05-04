use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
