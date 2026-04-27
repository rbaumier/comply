//! elysia-headers-lowercase backend — flag uppercase header keys inside a
//! route's `headers:` schema.

use crate::diagnostic::{Diagnostic, Severity};

const COMMON_UPPERCASE_KEYS: &[&str] = &[
    "Authorization:",
    "Content-Type:",
    "Accept:",
    "User-Agent:",
    "Cookie:",
    "X-",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    let Some(idx) = norm.find("headers:t.Object({") else { return };
    let after = &norm[idx..];

    // Bound the headers section to the next top-level key.
    let cut = ["body:", "params:", "query:", "response:", "cookie:", "detail:", "tags:"]
        .iter()
        .filter_map(|k| {
            // skip the `headers:` we just matched (idx 0)
            after[1..].find(k).map(|i| i + 1)
        })
        .min()
        .unwrap_or(after.len());
    let section = &after[..cut];

    let has_uppercase = COMMON_UPPERCASE_KEYS.iter().any(|k| section.contains(k));
    if !has_uppercase { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-headers-lowercase".into(),
        message: "`headers:` schema uses uppercase keys — Elysia lowercases header names, so the schema will never match.".into(),
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
    fn flags_uppercase_authorization() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ Authorization: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_uppercase_x_header() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ 'X-Api-Key': t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_lowercase_headers() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ authorization: t.String() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 'ok', { headers: t.Object({ Authorization: 1 }) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
