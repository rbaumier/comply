use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
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

fn extract_route_path<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let text = node.utf8_text(source).ok()?;
    match node.kind() {
        "string" => {
            let inner = text.trim_matches(|c| c == '"' || c == '\'');
            if inner.starts_with('/') {
                Some(inner)
            } else {
                None
            }
        }
        "template_string" => {
            if text.contains("${") {
                return None;
            }
            let inner = text.trim_matches('`');
            if inner.starts_with('/') {
                Some(inner)
            } else {
                None
            }
        }
        _ => None,
    }
}

const INFRA_PATHS: &[&str] = &[
    "/healthz", "/health", "/readyz", "/ready", "/livez", "/live", "/metrics",
];

fn is_infra_path(path: &str) -> bool {
    INFRA_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
        || path.starts_with("/dev/")
}

fn has_version_prefix(path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path);
    if !p.starts_with("/v") {
        return false;
    }
    let rest = &p[2..];
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digit_end == 0 {
        return false;
    }
    digit_end == rest.len() || rest.as_bytes()[digit_end] == b'/'
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    let Some(route_path) = extract_route_path(first, source) else { return };
    if has_version_prefix(route_path) || is_infra_path(route_path) {
        return;
    }
    let pos = first.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "api-route-version-prefix".into(),
        message: format!("Route `{}` does not start with a version prefix (e.g. /v1/…).", route_path).into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
                let d = crate::rules::test_helpers::run_rule(&Check, "app.get('/users', handler);", "src/routes.test.ts");
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
