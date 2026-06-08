use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "matchErrorPartial" {
            return;
        }

        // Without type info we can't know whether the union is fully enumerated.
        // Conservative heuristic: only flag when the match object enumerates
        // 3+ tags, suggesting the developer has covered most/all cases.
        let min_tags = ctx.config.threshold(
            "better-result-prefer-matcherror-exhaustive",
            "min_tags",
            ctx.lang,
        );

        // Find the first object argument.
        let Some(obj_expr) = call.arguments.iter().find_map(|arg| {
            match arg {
                Argument::ObjectExpression(obj) => Some(obj),
                _ => {
                    if let Some(Expression::ObjectExpression(obj)) = arg.as_expression() {
                        Some(obj)
                    } else {
                        None
                    }
                }
            }
        }) else {
            return;
        };

        let tag_count = obj_expr
            .properties
            .iter()
            .filter(|p| matches!(p, ObjectPropertyKind::ObjectProperty(_)))
            .count();

        if tag_count < min_tags {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer matchError (exhaustive) over matchErrorPartial when the union is fully enumerable.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_match_error_partial_with_three_tags() {
        let src = "result.matchErrorPartial({ NotFound: () => 0, NetworkError: () => 1, ParseError: () => 2 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_match_error_partial_with_one_tag() {
        let src = "result.matchErrorPartial({ NotFound: () => 0 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_match_error_partial_with_two_tags() {
        let src = "result.matchErrorPartial({ NotFound: () => 0, NetworkError: () => 1 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_match_error() {
        let src = "result.matchError({ NotFound: () => 0, NetworkError: () => 1 });";
        assert!(run(src).is_empty());
    }
}
