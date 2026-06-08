//! no-magic-array-flat-depth OXC backend — flag `arr.flat(N)` where N is a
//! numeric literal other than 1.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["flat"])
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
        if member.property.name.as_str() != "flat" {
            return;
        }
        let Some(first) = call.arguments.first() else {
            return;
        };
        let Argument::NumericLiteral(num) = first else {
            return;
        };
        // Allow depth 1 (the default).
        if (num.value - 1.0).abs() < f64::EPSILON {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Magic number as `.flat()` depth is not allowed. Use a named constant or `Infinity`.".into(),
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
    fn flags_magic_number_depth() {
        assert_eq!(run_on("arr.flat(3);").len(), 1);
    }


    #[test]
    fn flags_magic_number_depth_two() {
        assert_eq!(run_on("arr.flat(2);").len(), 1);
    }


    #[test]
    fn flags_magic_number_depth_large() {
        assert_eq!(run_on("const result = items.flat(10);").len(), 1);
    }


    #[test]
    fn allows_flat_without_args() {
        assert!(run_on("arr.flat();").is_empty());
    }


    #[test]
    fn allows_flat_depth_one() {
        assert!(run_on("arr.flat(1);").is_empty());
    }


    #[test]
    fn allows_flat_infinity() {
        assert!(run_on("arr.flat(Infinity);").is_empty());
    }


    #[test]
    fn allows_flat_variable() {
        assert!(run_on("arr.flat(depth);").is_empty());
    }


    #[test]
    fn ignores_comments() {
        assert!(run_on("// arr.flat(3);").is_empty());
    }
}
