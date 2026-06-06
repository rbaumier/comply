//! drizzle-serverless-pool-max-one — OXC backend.
//! Flag `new Pool({...})` in serverless contexts where `max` is not `1`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn is_serverless(path: &std::path::Path, source: &str) -> bool {
    let p = path.to_string_lossy();
    let path_hint = p.contains("/api/")
        || p.contains("lambda")
        || p.contains("edge")
        || p.contains("worker")
        || p.contains("functions/")
        || p.contains("netlify")
        || p.contains("cloudflare");
    let source_hint = crate::oxc_helpers::source_contains(source, "runtime = 'edge'")
        || crate::oxc_helpers::source_contains(source, "runtime: 'edge'")
        || crate::oxc_helpers::source_contains(source, "\"runtime\": \"edge\"")
        || crate::oxc_helpers::source_contains(source, "AWSLambda")
        || crate::oxc_helpers::source_contains(source, "APIGatewayProxyHandler")
        || crate::oxc_helpers::source_contains(source, "export const runtime");
    path_hint || source_hint
}

fn has_max_one(props: &oxc_allocator::Vec<'_, ObjectPropertyKind<'_>>) -> bool {
    props.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return false,
        };
        if key_name != "max" {
            return false;
        }
        matches!(&p.value, Expression::NumericLiteral(n) if (n.value - 1.0).abs() < f64::EPSILON)
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Pool"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Pool" {
            return;
        }

        if !is_serverless(ctx.path, ctx.source) {
            return;
        }

        // Check if any object argument has `max: 1`.
        let has_max = new_expr.arguments.iter().any(|arg| {
            if let Some(Expression::ObjectExpression(obj)) = arg.as_expression() {
                has_max_one(&obj.properties)
            } else {
                false
            }
        });
        if has_max {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Serverless `new Pool()` must set `max: 1` — each invocation has its own pool, >1 multiplies DB connections with concurrency.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "app/api/users/route.ts")
    }

    fn run_non_serverless(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "src/db.ts")
    }

    #[test]
    fn flags_pool_without_max_one_in_api() {
        let src = "const pool = new Pool({ connectionString: 'x' })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pool_with_max_one_in_api() {
        let src = "const pool = new Pool({ connectionString: 'x', max: 1 })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_serverless() {
        let src = "const pool = new Pool({ connectionString: 'x' })";
        assert!(run_non_serverless(src).is_empty());
    }
}
