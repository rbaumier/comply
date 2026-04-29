//! no-unvalidated-url-redirect backend — flag client-side navigations to
//! a URL sourced from user data: `window.location.href = userInput`,
//! `location.replace(userInput)`, `location.assign(userInput)`.

use crate::diagnostic::{Diagnostic, Severity};

const NAVIGATION_METHODS: &[&str] = &["replace", "assign"];

const LOCATION_SUFFIXES: &[&str] = &["location.href", "location"];

const USER_DATA_NEEDLES: &[&str] = &[
    "searchParams.get",
    "req.query",
    "req.params",
    "req.body",
    "params.",
    "query.",
];

fn text_references_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

fn is_location_target(text: &str) -> bool {
    LOCATION_SUFFIXES.iter().any(|s| text.ends_with(s))
}

fn is_location_navigation_call(name: &str) -> bool {
    let Some((receiver, method)) = name.rsplit_once('.') else {
        return false;
    };
    if !NAVIGATION_METHODS.contains(&method) {
        return false;
    }
    receiver.ends_with("location")
}

crate::ast_check! { on ["assignment_expression", "call_expression"] prefilter = ["location"] => |node, source, ctx, diagnostics|
match node.kind() {
        "assignment_expression" => {
            let Some(lhs) = node.child_by_field_name("left") else { return };
            let Ok(lhs_text) = lhs.utf8_text(source) else { return };
            if !is_location_target(lhs_text) {
                return;
            }
            let Some(rhs) = node.child_by_field_name("right") else { return };
            let Ok(rhs_text) = rhs.utf8_text(source) else { return };
            if !text_references_user_data(rhs_text) {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-unvalidated-url-redirect",
                "Client-side redirect target from user input — validate the URL before redirecting.".into(),
                Severity::Error,
            ));
        }
        "call_expression" => {
            let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
                return;
            };
            if !is_location_navigation_call(name) {
                return;
            }
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let Ok(args_text) = args.utf8_text(source) else { return };
            if text_references_user_data(args_text) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    "no-unvalidated-url-redirect",
                    "Client-side redirect target from user input — validate the URL before redirecting.".into(),
                    Severity::Error,
                ));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_location_href_from_search_params() {
        assert_eq!(
            run_on("window.location.href = searchParams.get('next')").len(),
            1
        );
    }

    #[test]
    fn flags_location_replace_with_query() {
        assert_eq!(run_on("location.replace(query.redirectUrl)").len(), 1);
    }

    #[test]
    fn allows_literal_location() {
        assert!(run_on("window.location.href = '/dashboard'").is_empty());
    }

    #[test]
    fn allows_validated_var() {
        assert!(run_on("window.location.href = safeUrl").is_empty());
    }
}
