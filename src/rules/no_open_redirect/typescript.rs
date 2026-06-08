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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
