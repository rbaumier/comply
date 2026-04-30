//! detect-dangerous-redirects backend — flag `res.redirect(req.…)` calls.
//!
//! Matches Express-style `*.redirect(...)` whose first (or second, for the
//! `(status, url)` form) argument is a member-expression rooted at `req`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `node` is a member expression whose root object is `req`,
/// e.g. `req.query.to`, `req.body.url`, `req.params.dest`.
fn is_req_member(mut node: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk down the object chain.
    loop {
        match node.kind() {
            "identifier" => {
                return node.utf8_text(source).unwrap_or("") == "req";
            }
            "member_expression" | "subscript_expression" => {
                let Some(obj) = node.child_by_field_name("object") else {
                    return false;
                };
                node = obj;
            }
            _ => return false,
        }
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    // We want `.redirect` — reject bare `redirect(...)` to avoid false positives on
    // unrelated functions; require a `.redirect` suffix so it's a method call.
    if !name.ends_with(".redirect") {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Check each positional arg — Express allows `(url)` or `(status, url)`.
    let mut cursor = args.walk();
    let tainted = args
        .named_children(&mut cursor)
        .any(|arg| is_req_member(arg, source));
    if !tainted {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "detect-dangerous-redirects".into(),
        message: "Redirecting to a value from `req` enables open-redirect attacks — validate against an allowlist first.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_redirect_req_query() {
        assert_eq!(run_on("res.redirect(req.query.to);").len(), 1);
    }

    #[test]
    fn flags_redirect_req_body() {
        assert_eq!(run_on("res.redirect(req.body.url);").len(), 1);
    }

    #[test]
    fn flags_redirect_with_status() {
        assert_eq!(run_on("res.redirect(302, req.query.url);").len(), 1);
    }

    #[test]
    fn allows_redirect_constant() {
        assert!(run_on(r#"res.redirect("/home");"#).is_empty());
    }

    #[test]
    fn allows_redirect_validated_var() {
        assert!(run_on("res.redirect(safeUrl);").is_empty());
    }
}
