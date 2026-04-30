//! elysia-group-deep-paths backend — flag deep route paths not using `.group()`.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

fn segment_count(path: &str) -> usize {
    path.split('/').filter(|s| !s.is_empty()).count()
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&method) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // First argument should be a string path.
    let Some(first_arg) = args.named_child(0) else { return };
    if first_arg.kind() != "string" {
        return;
    }
    let raw = first_arg.utf8_text(source).unwrap_or("");
    let unquoted = raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if segment_count(unquoted) < 3 {
        return;
    }

    // Skip if already inside a `.group(` call — best-effort check by walking parents.
    let mut p = node.parent();
    while let Some(parent) = p {
        if parent.kind() == "call_expression" {
            if let Some(pcallee) = parent.child_by_field_name("function") {
                if pcallee.kind() == "member_expression" {
                    if let Some(pprop) = pcallee.child_by_field_name("property") {
                        if pprop.utf8_text(source).unwrap_or("") == "group" {
                            return;
                        }
                    }
                }
            }
        }
        p = parent.parent();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-group-deep-paths".into(),
        message: format!(
            "Path `{unquoted}` has {} segments — consider grouping with `.group()` or a `prefix`.",
            segment_count(unquoted)
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_three_segment_path() {
        let src = "import { Elysia } from 'elysia';\napp.get('/v1/users/profile', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_four_segment_path() {
        let src = "import { Elysia } from 'elysia';\napp.post('/api/v2/users/me', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_shallow_path() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_grouped_routes() {
        let src = "import { Elysia } from 'elysia';\napp.group('/v1/users', g => g.get('/profile/edit/save', handler));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/v1/users/profile', handler);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
