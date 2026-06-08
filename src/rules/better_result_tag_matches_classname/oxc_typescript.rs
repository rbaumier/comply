//! better-result-tag-matches-classname — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["TaggedError"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        let Some(id) = &class.id else { return };
        let class_name = id.name.as_str();

        let Some(super_class) = &class.super_class else { return };
        let super_text =
            &ctx.source[super_class.span().start as usize..super_class.span().end as usize];
        if !super_text.contains("TaggedError") {
            return;
        }

        // Find TaggedError("...") call — the super_class is the call expression.
        let Expression::CallExpression(call) = super_class else { return };
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "TaggedError" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let arg_text =
            &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        let trimmed = arg_text
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim_matches('`');

        if trimmed != class_name {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, class.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "TaggedError tag '{trimmed}' does not match class name '{class_name}'."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_mismatched_tag() {
        let src = "class NotFoundError extends TaggedError('NotFound') {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matching_tag() {
        let src = "class NotFoundError extends TaggedError('NotFoundError') {}";
        assert!(run(src).is_empty());
    }
}
