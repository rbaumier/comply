//! OxcCheck backend — flag `Promise.reject()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "reject" {
            return;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "Promise" {
            return;
        }

        if forwards_caught_error(call, node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Promise.reject()` — prefer returning error values or throwing typed errors."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// `Promise.reject(catchVar)` in a `catch (catchVar)` body forwards an
/// already-caught exception rather than originating a fresh rejection, so it is
/// not flagged. The single argument must be a bare identifier equal to the
/// binding of the nearest enclosing `CatchClause`, reached before any function
/// boundary — a reject inside a nested closure references a different value and
/// stays flagged.
fn forwards_caught_error<'a>(
    call: &oxc_ast::ast::CallExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(oxc_ast::ast::Expression::Identifier(arg_ident)) =
        call.arguments.first().and_then(|arg| arg.as_expression())
    else {
        return false;
    };
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        match kind {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::CatchClause(catch) => {
                return catch
                    .param
                    .as_ref()
                    .and_then(|param| param.pattern.get_identifier_name())
                    .is_some_and(|name| name.as_str() == arg_ident.name.as_str());
            }
            _ => {}
        }
    }
    false
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
mod gated_tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_promise_reject_mock_fixture_in_test_dir() {
        // #5757 firing site (soft-delete-row-actions.test.tsx): a `vi.fn`
        // fixture rejecting to drive the component's error branch under test.
        // The rejected promise is the stimulus, not production error handling —
        // the central `skip_in_test_dir` gate must suppress it.
        let src = r#"const onDeactivateConfirm = vi.fn(() => Promise.reject(new Error("server error")));"#;
        assert!(
            run_rule_gated(&Check, src, "src/app/components/data-table/soft-delete-row-actions.test.tsx")
                .is_empty()
        );
    }

    #[test]
    fn flags_promise_reject_in_production() {
        // Negative-space guard: the same call in a production module is the
        // rule's genuine target — keep flagging.
        let src = r#"export function load() { return Promise.reject(new Error("boom")); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/load.ts").len(), 1);
    }

    #[test]
    fn skips_promise_reject_forwarding_catch_binding() {
        // #7164 (query-core retryer.ts): normalizing a synchronously-thrown
        // error into a rejected promise re-propagates the caught exception.
        let src = r#"export function run() {
            let promiseOrValue;
            try { promiseOrValue = fn(); } catch (error) { promiseOrValue = Promise.reject(error); }
            return promiseOrValue;
        }"#;
        assert!(run_rule_gated(&Check, src, "src/app/lib/retryer.ts").is_empty());
    }

    #[test]
    fn skips_void_promise_reject_forwarding_catch_binding() {
        // #7164 (query-core mutation.ts): re-raising a caught error as a global
        // unhandled rejection also forwards the caught exception.
        let src = r#"export async function onSettled() {
            try { await onSuccess(); } catch (e) { void Promise.reject(e); }
        }"#;
        assert!(run_rule_gated(&Check, src, "src/app/lib/mutation.ts").is_empty());
    }

    #[test]
    fn flags_promise_reject_of_non_catch_binding_in_catch() {
        // Argument is not the catch binding — this originates a rejection from
        // an unrelated value, so keep flagging.
        let src = r#"export function run(someOtherValue) {
            try { fn(); } catch (e) { return Promise.reject(someOtherValue); }
        }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/x.ts").len(), 1);
    }

    #[test]
    fn flags_promise_reject_of_new_error_in_catch() {
        // A fresh `new Error` inside a catch is a new rejection, not forwarding.
        let src = r#"export function run() {
            try { fn(); } catch (e) { return Promise.reject(new Error("boom")); }
        }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/v.ts").len(), 1);
    }

    #[test]
    fn flags_promise_reject_identifier_outside_catch() {
        // An ordinary identifier argument with no enclosing catch clause.
        let src = r#"export function run(e) { return Promise.reject(e); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/y.ts").len(), 1);
    }

    #[test]
    fn flags_promise_reject_in_nested_closure_inside_catch() {
        // Conservative scope: the reject sits inside a closure nested in the
        // catch body, so the caught binding is only closed over — keep flagging.
        let src = r#"export function run() {
            try { fn(); } catch (e) { run(() => Promise.reject(e)); }
        }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/z.ts").len(), 1);
    }

    #[test]
    fn flags_promise_reject_when_catch_has_no_binding() {
        // A bindingless `catch {}` cannot forward a caught error identifier.
        let src = r#"export function run(err) {
            try { fn(); } catch { return Promise.reject(err); }
        }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/w.ts").len(), 1);
    }
}
