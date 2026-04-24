//! security-require-oauth-state backend —
//! OAuth callback route handlers that never read/validate `state`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_oauth_callback_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/callback")
        || lower.contains("/oauth/callback")
        || lower.contains("/auth/callback")
        || lower.ends_with("/cb")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    let is_route_reg = name.ends_with(".get")
        || name.ends_with(".post")
        || name.ends_with(".all");
    if !is_route_reg {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    let Some(path_node) = positional.first() else {
        return;
    };
    if path_node.kind() != "string" {
        return;
    }
    let Ok(path_text) = path_node.utf8_text(source) else {
        return;
    };
    if !is_oauth_callback_path(path_text) {
        return;
    }

    // Look at the handler body text — it must mention `state`.
    let mut reads_state = false;
    for arg in positional.iter().skip(1) {
        let Ok(text) = arg.utf8_text(source) else {
            continue;
        };
        if text.contains("state") {
            reads_state = true;
            break;
        }
    }
    if reads_state {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "OAuth callback handler {path_text} never reads `state` — CSRF validation is missing."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_callback_without_state() {
        let src = "app.get('/auth/callback', (req, res) => { exchange(req.query.code); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_callback_validating_state() {
        let src = "app.get('/auth/callback', (req, res) => { if (req.query.state !== saved) throw 0; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_callback_paths() {
        assert!(run("app.get('/widgets', listWidgets);").is_empty());
    }
}
