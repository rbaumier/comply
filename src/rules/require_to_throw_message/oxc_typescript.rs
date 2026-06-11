//! require-to-throw-message — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::Span;
use std::sync::Arc;

/// Schema-validation methods whose rejection is the test contract: a bare
/// `.toThrow()` on `expect(() => schema.parse(x))` asserts "invalid input is
/// rejected", and pinning the exact message is brittle across library
/// versions. Covers zod/valibot (`parse`/`parseAsync`), yup
/// (`validate`/`validateSync`/`cast`), and joi (`validate`/`attempt`).
const VALIDATION_METHODS: &[&str] =
    &["parse", "parseAsync", "validate", "validateSync", "cast", "attempt"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toThrow"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let name = member.property.name.as_str();
        if name != "toThrow" && name != "toThrowError" {
            return;
        }

        // Skip `.not.toThrow()` / `.not.toThrowError()` — asserts no error; no argument needed
        if let Expression::StaticMemberExpression(obj_member) = &member.object {
            if obj_member.property.name.as_str() == "not" {
                return;
            }
        }

        // Flag only when called with zero arguments.
        if !call.arguments.is_empty() {
            return;
        }

        // Skip schema-validation rejection tests: the expect() subject is a
        // callback that invokes a validation method (e.g. `schema.parse`).
        if is_validation_rejection_subject(&member.object, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Provide expected error message to toThrow().".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns true when the `.toThrow()` receiver is an `expect(...)` call (the
/// `expect(...)` may be wrapped in member chains like `.rejects`/`.resolves`)
/// whose first argument is a callback invoking a schema-validation method.
fn is_validation_rejection_subject<'a>(
    object: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = object;
    let expect_call = loop {
        match current {
            Expression::StaticMemberExpression(member) => current = &member.object,
            Expression::CallExpression(call) => break call,
            _ => return false,
        }
    };
    let Expression::Identifier(callee) = &expect_call.callee else { return false };
    if callee.name.as_str() != "expect" {
        return false;
    }

    let callback_span = match expect_call.arguments.first() {
        Some(Argument::ArrowFunctionExpression(arrow)) => arrow.span,
        Some(Argument::FunctionExpression(func)) => func.span,
        _ => return false,
    };

    semantic.nodes().iter().any(|node| {
        let AstKind::CallExpression(inner) = node.kind() else { return false };
        if !contains_span(callback_span, inner.span) {
            return false;
        }
        callee_property_name(&inner.callee)
            .is_some_and(|name| VALIDATION_METHODS.contains(&name))
    })
}

fn contains_span(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// Property name of a member-expression callee (`x.parse(...)` or
/// `x["parse"](...)`); `None` for non-member callees.
fn callee_property_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        Expression::ComputedMemberExpression(member) => match &member.expression {
            Expression::StringLiteral(lit) => Some(lit.value.as_str()),
            _ => None,
        },
        _ => None,
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Regression for issue #993 — zod's own test suite (relative imports, so
    // exemption must be shape-based, not import-based).
    #[test]
    fn exempts_zod_parse_rejection() {
        let d = run_on(
            r#"expect(() => stringSchema.parse({ constructor: 123, key: "value" })).toThrow();"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_parse_async_rejection() {
        let d = run_on("expect(() => schema.parseAsync(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_validate_rejection() {
        let d = run_on("expect(() => schema.validate(x)).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_block_bodied_callback() {
        let d = run_on("expect(() => { schema.parse(x); }).toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_awaited_parse_async_behind_rejects() {
        let d = run_on("expect(async () => { await schema.parseAsync(x); }).rejects.toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_bare_call_in_callback() {
        let d = run_on("expect(() => doStuff()).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_validation_member_call() {
        let d = run_on("expect(() => service.compute()).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_callback_subject() {
        let d = run_on("expect(myThrowingFn).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_not_to_throw() {
        let d = run_on("expect(() => doStuff()).not.toThrow();");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_to_throw_with_message() {
        let d = run_on(r#"expect(() => doStuff()).toThrow("boom");"#);
        assert!(d.is_empty());
    }
}
