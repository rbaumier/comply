//! api-route-version-prefix oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

const INFRA_PATHS: &[&str] = &[
    "/healthz", "/health", "/readyz", "/ready", "/livez", "/live", "/metrics",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.contains(".test.") || name.contains(".spec.") {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("__tests__") | Some("__test__") | Some("tests") | Some("test")
        )
    })
}

fn is_infra_path(path: &str) -> bool {
    INFRA_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
        || path.starts_with("/dev/")
}

fn has_version_prefix(path: &str) -> bool {
    if !path.starts_with("/v") {
        return false;
    }
    let rest = &path[2..];
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digit_end == 0 {
        return false;
    }
    digit_end == rest.len() || rest.as_bytes()[digit_end] == b'/'
}

fn extract_route_path<'a>(expr: &'a Expression<'a>, source: &'a str) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(lit) => {
            let s = lit.value.as_str();
            if s.starts_with('/') { Some(s) } else { None }
        }
        Expression::TemplateLiteral(tpl) => {
            if !tpl.expressions.is_empty() {
                return None;
            }
            let text = &source[tpl.span.start as usize + 1..tpl.span.end as usize - 1];
            if text.starts_with('/') { Some(text) } else { None }
        }
        _ => None,
    }
}

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

        if is_test_file(ctx.path) {
            return;
        }

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let name = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&name) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(first_expr) = first_arg.as_expression() else {
            return;
        };
        let Some(route_path) = extract_route_path(first_expr, ctx.source) else {
            return;
        };
        if has_version_prefix(route_path) || is_infra_path(route_path) {
            return;
        }

        let span = first_expr.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Route `{route_path}` does not start with a version prefix (e.g. /v1/\u{2026})."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_route_without_version() {
        let d = run("app.get('/users', handler);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/users"));
    }

    #[test]
    fn flags_post_without_version() {
        assert_eq!(run("router.post('/items', handler);").len(), 1);
    }

    #[test]
    fn flags_template_string_without_version() {
        assert_eq!(run("app.get(`/users`, handler);").len(), 1);
    }

    #[test]
    fn allows_versioned_route() {
        assert!(run("app.get('/v1/users', handler);").is_empty());
    }

    #[test]
    fn allows_v2() {
        assert!(run("app.put('/v2/items', handler);").is_empty());
    }

    #[test]
    fn allows_versioned_template_string() {
        assert!(run("app.delete(`/v1/users`, handler);").is_empty());
    }

    #[test]
    fn ignores_non_route_string() {
        assert!(run("app.get('someKey', handler);").is_empty());
    }

    #[test]
    fn ignores_dynamic_template() {
        assert!(run("app.get(`/users/${id}`, handler);").is_empty());
    }

    #[test]
    fn ignores_non_http_method() {
        assert!(run("map.set('/users', value);").is_empty());
    }

    #[test]
    fn ignores_plain_function_call() {
        assert!(run("get('/users');").is_empty());
    }

    #[test]
    fn ignores_test_file() {
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(
            "app.get('/users', handler);",
            &Check,
            "src/routes.test.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_route_method() {
        assert_eq!(run("app.route('/users').get(handler);").len(), 1);
    }

    #[test]
    fn rejects_v_without_digit() {
        assert_eq!(run("app.get('/vx/users', handler);").len(), 1);
    }

    #[test]
    fn allows_version_only_path() {
        assert!(run("app.get('/v1', handler);").is_empty());
    }

    #[test]
    fn allows_healthz() {
        assert!(run("app.get('/healthz', handler);").is_empty());
    }

    #[test]
    fn allows_health_check_variants() {
        assert!(run("app.get('/health', handler);").is_empty());
        assert!(run("app.get('/readyz', handler);").is_empty());
        assert!(run("app.get('/livez', handler);").is_empty());
        assert!(run("app.get('/metrics', handler);").is_empty());
    }

    #[test]
    fn allows_dev_endpoints() {
        assert!(run("app.get('/dev/last-reset-url', handler);").is_empty());
    }
}
