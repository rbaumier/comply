//! no-unvalidated-url-redirect OXC backend — flag client-side navigations
//! to a URL sourced from user data.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const NAVIGATION_METHODS: &[&str] = &["replace", "assign"];

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
    text.ends_with("location.href") || text.ends_with("location")
}

fn is_location_navigation_method(receiver: &str, method: &str) -> bool {
    NAVIGATION_METHODS.contains(&method) && receiver.ends_with("location")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let lhs_text = &ctx.source
                    [assign.left.span().start as usize..assign.left.span().end as usize];
                if !is_location_target(lhs_text) {
                    return;
                }
                let rhs_text = &ctx.source
                    [assign.right.span().start as usize..assign.right.span().end as usize];
                if !text_references_user_data(rhs_text) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Client-side redirect target from user input — validate the URL before redirecting.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let method = member.property.name.as_str();
                let obj_text = &ctx.source
                    [member.object.span().start as usize..member.object.span().end as usize];
                if !is_location_navigation_method(obj_text, method) {
                    return;
                }
                let args_text = &ctx.source
                    [call.span.start as usize..call.span.end as usize];
                if !text_references_user_data(args_text) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Client-side redirect target from user input — validate the URL before redirecting.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
