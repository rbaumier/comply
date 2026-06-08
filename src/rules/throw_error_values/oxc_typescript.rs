//! throw-error-values — OXC backend.
//! Flag `throw` of non-Error values (primitives, objects, arrays, templates).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_non_error_throw(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::RegExpLiteral(_) => true,
        Expression::ParenthesizedExpression(p) => is_non_error_throw(&p.expression),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };

        if !is_non_error_throw(&throw.argument) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Throw an `Error` instance, not a primitive or plain object — \
                      non-Error throws lose stack traces and break `instanceof` checks."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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

    #[test]
    fn flags_throw_string_literal() {
        let d = run_on("function f() { throw 'boom'; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "throw-error-values");
    }

    #[test]
    fn flags_throw_template_string() {
        let d = run_on("function f() { throw `boom ${x}`; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_number() {
        let d = run_on("function f() { throw 42; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_object_literal() {
        let d = run_on("function f() { throw { code: 500 }; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_array() {
        let d = run_on("function f() { throw [1, 2]; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_null() {
        let d = run_on("function f() { throw null; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_throw_new_error() {
        assert!(run_on("function f() { throw new Error('boom'); }").is_empty());
    }

    #[test]
    fn allows_throw_identifier() {
        assert!(run_on("function f(e) { throw e; }").is_empty());
    }

    #[test]
    fn allows_throw_call_expression() {
        assert!(run_on("function f() { throw makeError(); }").is_empty());
    }

    #[test]
    fn allows_throw_member() {
        assert!(run_on("function f(e) { throw e.cause; }").is_empty());
    }
}
