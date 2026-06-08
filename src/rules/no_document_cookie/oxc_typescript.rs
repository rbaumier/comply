//! no-document-cookie oxc backend — flag direct `document.cookie` access.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
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
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };
        if member.property.name.as_str() != "cookie" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "document" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not use `document.cookie` directly — use a cookie library instead.".into(),
            severity: Severity::Warning,
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
