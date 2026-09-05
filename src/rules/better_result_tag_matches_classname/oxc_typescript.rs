//! better-result-tag-matches-classname — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

/// The `TaggedError("tag")` call a class extends, if any.
/// v3 writes `extends TaggedError("tag")`.
/// v2 writes `extends TaggedError("tag")<Props>()`.
/// So the call sits under zero or more callee layers.
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

/// Whether a tag and a class name spell one error.
/// A wire-code tag drops the `Error` suffix.
/// It also starts lowercase: `notFound` names `NotFoundError`.
fn spells_same_error(tag: &str, class_name: &str) -> bool {
    without_error_suffix(tag).eq_ignore_ascii_case(without_error_suffix(class_name))
}

/// `name` stripped of a trailing `Error`, in any case.
/// Returns `name` itself when that suffix is all there is.
fn without_error_suffix(name: &str) -> &str {
    const SUFFIX: &str = "Error";
    let Some(cut) = name.len().checked_sub(SUFFIX.len()).filter(|&cut| cut > 0) else {
        return name;
    };
    match name.get(cut..) {
        Some(suffix) if suffix.eq_ignore_ascii_case(SUFFIX) => &name[..cut],
        _ => name,
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

        // A runtime-built tag has no name to compare.
        let Some(Argument::StringLiteral(literal)) = call.arguments.first() else { return };
        let tag = literal.value.as_str();

        if !spells_same_error(tag, class_name) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, class.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "TaggedError tag '{tag}' does not name class '{class_name}'."
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

    /// Issue #8481 — v2 declares the subclass through a factory call.
    /// Reading it keeps a v2 project from scanning silently clean.
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

    /// Issue #8482 — an RFC 7807 API tags with the wire `code`.
    /// That code is the class name minus its `Error` suffix.
    #[test]
    fn allows_tag_that_is_the_class_name_without_its_error_suffix() {
        for src in [
            r#"export class NotFoundError extends TaggedError("notFound")<{}>() {}"#,
            r#"export class ConflictError extends TaggedError("conflict") {}"#,
            r#"export class RateLimitedError extends TaggedError("rateLimited") {}"#,
            r#"export class EmailNotVerifiedError extends TaggedError("emailNotVerified") {}"#,
            r#"export class OrganizationHasAttachedTeamsError extends TaggedError("organizationHasAttachedTeams") {}"#,
        ] {
            assert!(run(src).is_empty(), "{src}");
        }
    }

    #[test]
    fn flags_typo_in_a_camel_case_tag() {
        let src = r#"export class NotFoundError extends TaggedError("notFoundd") {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tag_copied_from_another_error() {
        let src = r#"export class ConflictError extends TaggedError("notFound") {}"#;
        assert_eq!(run(src).len(), 1);
    }
}
