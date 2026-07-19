//! elysia-static-inline-value OXC backend — flag arrow handlers that only
//! return a string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FormalParameterKind, Statement};
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

fn is_string_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(_) | Expression::TemplateLiteral(_))
}

fn arrow_returns_only_string(arrow: &oxc_ast::ast::ArrowFunctionExpression) -> bool {
    // Expression body: `() => "literal"`
    if arrow.expression {
        let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
            return false;
        };
        return is_string_literal(&stmt.expression);
    }
    // Block body with a single return statement.
    let stmts: Vec<_> = arrow.body.statements.iter().collect();
    if stmts.len() != 1 {
        return false;
    }
    let Statement::ReturnStatement(ret) = stmts[0] else { return false };
    ret.argument.as_ref().is_some_and(|arg| is_string_literal(arg))
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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // The recommended inline-literal form (`.get('/health', 'ok')`) is
        // exactly what `elysia-cf-no-inline-values` (Error) forbids on the
        // Cloudflare Workers adapter, where AOT-inlining a string handler is
        // broken. On a Cloudflare target the two rules would be mutually
        // contradictory — no call shape satisfies both — so this one backs off
        // and lets the Error-severity cf rule govern (#5753).
        if ctx.project.is_cloudflare_target() {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if !ROUTE_METHODS.contains(&member.property.name.as_str()) {
            return;
        }

        if call.arguments.len() < 2 {
            return;
        }
        let Argument::ArrowFunctionExpression(arrow) = &call.arguments[1] else {
            return;
        };

        // Bail if the arrow takes any parameters.
        if arrow.params.kind == FormalParameterKind::FormalParameter
            && !arrow.params.items.is_empty()
        {
            return;
        }

        if !arrow_returns_only_string(arrow) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Handler returns only a static string \u{2014} pass the literal directly so Elysia can compile it ahead of time.".into(),
            severity: Severity::Error,
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
    use crate::project::ProjectCtx;
    use std::path::Path;
    use tempfile::TempDir;

    fn cf_project(dir: &Path) -> ProjectCtx {
        std::fs::write(dir.join("wrangler.toml"), "name = \"x\"\n").unwrap();
        let mut ctx = ProjectCtx::for_test_with_framework("elysia");
        ctx.project_root = Some(dir.to_path_buf());
        ctx
    }

    fn non_cf_project(dir: &Path) -> ProjectCtx {
        let mut ctx = ProjectCtx::for_test_with_framework("elysia");
        ctx.project_root = Some(dir.to_path_buf());
        ctx
    }

    fn run_in_project(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            "t.ts",
            project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_arrow_returning_string_on_non_cf_project() {
        // On a Bun/Node/K8s target the inline literal is valid and preferred,
        // so the arrow-returning-only-a-string handler is flagged.
        let dir = TempDir::new().unwrap();
        let src = r#"app.get("/health", () => "ok")"#;
        assert_eq!(run_in_project(src, &non_cf_project(dir.path())).len(), 1);
    }

    #[test]
    fn ignores_arrow_returning_string_on_cloudflare_target() {
        // #5753: on a Cloudflare target the inline form this rule recommends is
        // exactly what cf-no-inline-values forbids, so this rule backs off.
        let dir = TempDir::new().unwrap();
        let src = r#"app.get("/health", () => "ok")"#;
        assert!(
            run_in_project(src, &cf_project(dir.path())).is_empty(),
            "{:?}",
            run_in_project(src, &cf_project(dir.path())),
        );
    }

    #[test]
    fn ignores_handler_with_logic_on_non_cf_project() {
        // A handler doing real work (not just returning a literal) is never the
        // rule's target, even on a non-Cloudflare project.
        let dir = TempDir::new().unwrap();
        let src = r#"app.get("/health", () => { check(); return "ok"; })"#;
        assert!(run_in_project(src, &non_cf_project(dir.path())).is_empty());
    }
}
