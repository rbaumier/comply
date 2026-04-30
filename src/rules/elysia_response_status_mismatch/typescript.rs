//! elysia-response-status-mismatch backend — flag handlers returning status codes not in `response:` schema.

use crate::diagnostic::{Diagnostic, Severity};

const STATUSES: &[&str] = &["401", "403", "404", "409", "500"];
const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

fn extract_response_block(args_text: &str) -> Option<&str> {
    let idx = args_text.find("response:")?;
    let after = &args_text[idx + "response:".len()..];
    let after = after.trim_start();
    if !after.starts_with('{') {
        return None;
    }
    // Walk braces.
    let bytes = after.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after[..=i]);
                }
            }
            _ => {}
        }
    }
    None
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
    let args_text = args.utf8_text(source).unwrap_or("");
    let Some(response_block) = extract_response_block(args_text) else { return };

    // For each common status, check if handler uses `status(N` but the response block lacks `N:`.
    for code in STATUSES {
        let status_call = format!("status({code}");
        if !args_text.contains(&status_call) {
            continue;
        }
        let key = format!("{code}:");
        if response_block.contains(&key) {
            continue;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-response-status-mismatch".into(),
            message: format!("Handler returns `status({code}, ...)` but `response:` schema has no `{code}:` key."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_404_not_in_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/u', ({ status }) => status(404, 'nope'), { response: { 200: t.String() } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_401_not_in_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.post('/login', ({ status }) => status(401, 'no'), { response: { 200: t.String() } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_when_status_in_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/u', ({ status }) => status(404, 'nope'), { response: { 200: t.String(), 404: t.String() } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_when_no_response_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.get('/u', ({ status }) => status(404, 'nope'));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/u', () => status(404, 'nope'), { response: { 200: 'x' } });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
