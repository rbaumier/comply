//! prefer-array-from-map oxc backend — flag `[...iter].map(fn)`.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Look for [...iter].map(fn)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "map" {
            return;
        }

        let Expression::ArrayExpression(arr) = &member.object else { return };

        // Check if array is [...something] (exactly one spread element)
        if arr.elements.len() != 1 {
            return;
        }
        let ArrayExpressionElement::SpreadElement(spread) = &arr.elements[0] else { return };

        // Skip if spreading an array literal
        if matches!(&spread.argument, Expression::ArrayExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `Array.from(iter, mapFn)` instead of `[...iter].map(mapFn)`.".into(),
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
    fn flags_spread_map() {
        assert_eq!(run("[...set].map(x => x * 2)").len(), 1);
        assert_eq!(run("[...iter].map(fn)").len(), 1);
    }


    #[test]
    fn allows_array_from() {
        assert!(run("Array.from(set, x => x * 2)").is_empty());
    }


    #[test]
    fn allows_array_literal_map() {
        assert!(run("[1, 2, 3].map(x => x * 2)").is_empty());
    }


    #[test]
    fn allows_variable_map() {
        assert!(run("arr.map(x => x * 2)").is_empty());
    }
}
