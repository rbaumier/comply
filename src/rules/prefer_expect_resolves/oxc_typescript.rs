//! prefer-expect-resolves OXC backend — flag `expect(await promise)` calls.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};

pub struct Check;

/// True when the awaited expression is a React-Testing-Library `findBy*` /
/// `findAllBy*` query. Those reject (throw) on not-found with RTL's own
/// diagnostic message, so `expect(await screen.findByText(...))` already fails
/// helpfully — rewriting to `.resolves` is no improvement and breaks the
/// canonical RTL idiom.
fn awaited_is_rtl_find_query(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return false;
    };
    let name = match &call.callee {
        Expression::StaticMemberExpression(m) => m.property.name.as_str(),
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    ["findBy", "findAllBy"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_uppercase())
    })
}

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

        // Callee must be the identifier `expect`.
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "expect" {
            return;
        }

        // Must have exactly one argument, and it must be an await expression.
        if call.arguments.len() != 1 {
            return;
        }
        let Argument::AwaitExpression(await_expr) = &call.arguments[0] else { return };

        // RTL `findBy*` queries already reject on miss — no `.resolves` gain.
        if awaited_is_rtl_find_query(&await_expr.argument) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `await expect(promise).resolves` instead of `expect(await promise)`.".into(),
            severity: Severity::Warning,
            span: Some((call.span.start as usize, (call.span.end - call.span.start) as usize)),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_expect_await_value() {
        assert_eq!(run("expect(await getValue()).toEqual(1);").len(), 1);
    }

    // Regression for #270: RTL `findBy*`/`findAllBy*` queries reject on miss,
    // so `expect(await screen.findByText(...))` is the canonical idiom.
    #[test]
    fn skips_rtl_find_by_query() {
        let src = r#"expect(await screen.findByText("Mot de passe trop court.")).toBeInTheDocument();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_rtl_find_all_by_query() {
        let src = r#"expect(await screen.findAllByRole("button")).toHaveLength(2);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_bare_find_by_import() {
        assert!(run(r#"expect(await findByTestId("x")).toBeVisible();"#).is_empty());
    }
}
