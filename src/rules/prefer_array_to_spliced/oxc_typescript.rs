//! OxcCheck backend — flag copy-then-splice patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["splice"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be `*.splice`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "splice" {
            return;
        }
        // Check if the object is a copy pattern: .slice() or [...arr]
        let is_copy_pattern = match &member.object {
            Expression::CallExpression(inner_call) => {
                // arr.slice().splice()
                if let Expression::StaticMemberExpression(inner_member) = &inner_call.callee {
                    inner_member.property.name.as_str() == "slice"
                } else {
                    false
                }
            }
            Expression::ArrayExpression(arr) => {
                // [...arr].splice()
                arr.elements.len() == 1
                    && matches!(arr.elements[0], ArrayExpressionElement::SpreadElement(_))
            }
            _ => false,
        };
        if !is_copy_pattern {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `toSpliced()` instead of copy-then-splice pattern.".into(),
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
    fn flags_slice_splice() {
        assert_eq!(run("arr.slice().splice(1, 2)").len(), 1);
    }


    #[test]
    fn flags_spread_splice() {
        assert_eq!(run("[...arr].splice(1, 2)").len(), 1);
    }


    #[test]
    fn allows_direct_splice() {
        assert!(run("arr.splice(1, 2)").is_empty());
    }


    #[test]
    fn allows_to_spliced() {
        assert!(run("arr.toSpliced(1, 2)").is_empty());
    }
}
