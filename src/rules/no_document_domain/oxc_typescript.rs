//! no-document-domain oxc backend — flag `document.domain = ...` assignments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
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
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };
        let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };
        if member.property.name.as_str() != "domain" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "document" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assigning to `document.domain` weakens the same-origin policy — remove it and use `postMessage` or CORS instead.".into(),
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
