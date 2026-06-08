//! prefer-modern-dom-apis oxc backend — flag legacy DOM mutation methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const PATTERNS: &[(&str, &str)] = &[
    (
        "insertBefore",
        "Prefer `ref.before(newNode)` over `parent.insertBefore(newNode, ref)`.",
    ),
    (
        "replaceChild",
        "Prefer `old.replaceWith(newNode)` over `parent.replaceChild(newNode, old)`.",
    ),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["insertBefore", "replaceChild"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let name = member.property.name.as_str();
        let Some((_, message)) = PATTERNS.iter().find(|(p, _)| *p == name) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: (*message).into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_insert_before() {
        let d = run_oxc_ts("parent.insertBefore(newNode, refNode);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }


    #[test]
    fn flags_replace_child() {
        let d = run_oxc_ts("parent.replaceChild(newEl, oldEl);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }


    #[test]
    fn allows_modern_before() {
        assert!(run_oxc_ts("refNode.before(newNode);").is_empty());
    }


    #[test]
    fn allows_modern_replace_with() {
        assert!(run_oxc_ts("oldEl.replaceWith(newEl);").is_empty());
    }


    #[test]
    fn ignores_comment() {
        assert!(run_oxc_ts("// parent.insertBefore(a, b)").is_empty());
    }
}
