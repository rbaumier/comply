//! better-result-tag-matches-classname — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

/// The `TaggedError("tag")` call a class extends, or `None` when the class
/// extends anything else. better-result 3.x spells the heritage clause
/// `extends TaggedError("tag")`, 2.x wraps it in a factory call —
/// `extends TaggedError("tag")<Props>()` — so the tagging call can sit one or
/// more callee layers below the heritage expression.
fn tagged_error_call<'a>(heritage: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
    let mut expr = heritage.get_inner_expression();
    loop {
        let Expression::CallExpression(call) = expr else { return None };
        let callee = call.callee.get_inner_expression();
        if matches!(callee, Expression::Identifier(id) if id.name == "TaggedError") {
            return Some(call);
        }
        expr = callee;
    }
}

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
        let Some(call) = tagged_error_call(super_class) else { return };

        // A tag built at runtime carries no name to compare against.
        let Some(Argument::StringLiteral(literal)) = call.arguments.first() else { return };
        let tag = literal.value.as_str();

        if tag != class_name {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, class.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "TaggedError tag '{tag}' does not match class name '{class_name}'."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_mismatched_tag() {
        let src = r#"export class NotFoundError extends TaggedError("userGone") {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matching_tag() {
        let src = r#"export class NotFoundError extends TaggedError("NotFoundError") {}"#;
        assert!(run(src).is_empty());
    }

    /// Issue #8481 — better-result 2.x declares the subclass through a factory
    /// call, so the heritage expression is `TaggedError("tag")<Props>()`. The
    /// rule must read the tag through that extra call layer instead of going
    /// silent on the whole project.
    #[test]
    fn flags_mismatched_tag_in_v2_factory_syntax() {
        let src = r#"export class NotFoundError extends TaggedError("userGone")<{}>() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matching_tag_in_v2_factory_syntax() {
        let src = r#"export class NotFoundError extends TaggedError("NotFoundError")<{ id: string }>() {}"#;
        assert!(run(src).is_empty());
    }
}
