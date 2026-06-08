//! js-no-math-spread-array OXC backend — flag `Math.min(...arr)` / `Math.max(...arr)`.

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
        Some(&["Math"])
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
        if obj.name.as_str() != "Math" {
            return;
        }
        let method = member.property.name.as_str();
        if method != "min" && method != "max" {
            return;
        }
        let has_spread = call.arguments.iter().any(|a| matches!(a, Argument::SpreadElement(_)));
        if !has_spread {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`Math.{method}(...array)` overflows the stack on large arrays — \
                 use `reduce` or a for-loop instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_math_min_spread() {
        assert_eq!(run(r#"const m = Math.min(...values);"#).len(), 1);
    }


    #[test]
    fn flags_math_max_spread() {
        assert_eq!(run(r#"const m = Math.max(...values);"#).len(), 1);
    }


    #[test]
    fn flags_math_max_spread_with_other_args() {
        assert_eq!(run(r#"const m = Math.max(0, ...values);"#).len(), 1);
    }


    #[test]
    fn allows_math_min_literal_args() {
        assert!(run(r#"const m = Math.min(1, 2, 3);"#).is_empty());
    }


    #[test]
    fn allows_math_max_literal_args() {
        assert!(run(r#"const m = Math.max(a, b);"#).is_empty());
    }


    #[test]
    fn allows_other_math_with_spread() {
        assert!(run(r#"const m = Math.hypot(...values);"#).is_empty());
    }
}
