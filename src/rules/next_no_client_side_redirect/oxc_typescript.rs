//! next-no-client-side-redirect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn report(span_start: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, msg: &str) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: msg.into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Check if expression is `window.location` or bare `location`.
fn is_window_location_target(expr: &Expression) -> bool {
    if let Expression::StaticMemberExpression(member) = expr
        && let Expression::Identifier(id) = &member.object
            && id.name.as_str() == "window" && member.property.name.as_str() == "location" {
                return true;
            }
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == "location")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["location"])
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
                // Use source text of the assignment target for simple matching.
                let target_text = &ctx.source
                    [assign.left.span().start as usize..assign.left.span().end as usize];

                if target_text == "window.location" {
                    report(
                        assign.span.start,
                        ctx,
                        diagnostics,
                        "Assigning to `window.location` triggers a full page reload \u{2014} use Next.js `redirect()` or `useRouter().push()`.",
                    );
                    return;
                }

                if target_text == "window.location.href" || target_text == "location.href" {
                    report(
                        assign.span.start,
                        ctx,
                        diagnostics,
                        "Assigning to `location.href` triggers a full page reload \u{2014} use Next.js `redirect()` or `useRouter().push()`.",
                    );
                }
            }
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(callee) = &call.callee else {
                    return;
                };
                let method = callee.property.name.as_str();
                if method != "replace" && method != "assign" {
                    return;
                }
                if !is_window_location_target(&callee.object) {
                    return;
                }
                report(
                    call.span.start,
                    ctx,
                    diagnostics,
                    &format!("`location.{method}()` triggers a full page reload \u{2014} use Next.js `redirect()` or `useRouter().push()`."),
                );
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_window_location_assignment() {
        let diags = run("function f() { window.location = '/home'; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("window.location"));
    }

    #[test]
    fn flags_window_location_href_assignment() {
        let diags = run("function f() { window.location.href = '/home'; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.href"));
    }

    #[test]
    fn flags_location_href_assignment() {
        let diags = run("function f() { location.href = '/home'; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_window_location_replace_call() {
        let diags = run("function f() { window.location.replace('/home'); }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.replace"));
    }

    #[test]
    fn flags_location_assign_call() {
        let diags = run("function f() { location.assign('/home'); }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("location.assign"));
    }

    #[test]
    fn allows_router_push() {
        assert!(run("function f(router) { router.push('/home'); }").is_empty());
    }

    #[test]
    fn allows_redirect_call() {
        assert!(run("function f() { redirect('/home'); }").is_empty());
    }

    #[test]
    fn allows_unrelated_replace() {
        assert!(run("function f(s) { return s.replace('a', 'b'); }").is_empty());
    }
}
