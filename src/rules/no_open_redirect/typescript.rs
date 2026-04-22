//! no-open-redirect backend — flag `res.redirect(userInput)` calls whose
//! argument references request-scoped data (query/params/body).

use crate::diagnostic::{Diagnostic, Severity};

const REDIRECT_METHODS: &[&str] = &["redirect"];

const USER_DATA_NEEDLES: &[&str] = &[
    "req.query",
    "req.params",
    "req.body",
    "request.query",
    "request.params",
    "request.body",
    "searchParams.get",
];

fn is_redirect_call(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    REDIRECT_METHODS.contains(&tail)
}

fn argument_references_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_redirect_call(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        let Ok(text) = arg.utf8_text(source) else { continue };
        if argument_references_user_data(text) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-open-redirect",
                "Redirect target from user input — validate against an allowlist before redirecting.".into(),
                Severity::Error,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_redirect_with_query_param() {
        assert_eq!(run_on("res.redirect(req.query.returnUrl)").len(), 1);
    }

    #[test]
    fn flags_redirect_with_search_params() {
        assert_eq!(run_on("res.redirect(searchParams.get('next'))").len(), 1);
    }

    #[test]
    fn allows_literal_redirect() {
        assert!(run_on("res.redirect('/dashboard')").is_empty());
    }

    #[test]
    fn allows_validated_redirect() {
        assert!(run_on("res.redirect(safeUrl)").is_empty());
    }
}
