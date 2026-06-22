//! promise-catch-or-return oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// A `.then(onFulfilled, onRejected)` call handles rejection itself: the
/// second argument is the rejection handler per the Promises/A+ spec. Returns
/// true when the call passes a second argument that is not literally
/// `undefined` / `null` (which would be a no-op rejection handler).
fn then_has_rejection_handler(call: &CallExpression) -> bool {
    let Some(second) = call.arguments.get(1) else {
        return false;
    };
    let Argument::SpreadElement(_) = second else {
        // A non-spread argument is always an `Expression`. It's a no-op handler
        // only when it's literally `undefined`, `null`, or `void <expr>`.
        let is_noop = matches!(
            second.as_expression().map(Expression::get_inner_expression),
            Some(Expression::Identifier(id)) if id.name == "undefined"
        ) || matches!(
            second.as_expression().map(Expression::get_inner_expression),
            Some(Expression::NullLiteral(_))
        ) || matches!(
            second.as_expression().map(Expression::get_inner_expression),
            Some(Expression::UnaryExpression(u)) if u.operator == UnaryOperator::Void
        );
        return !is_noop;
    };
    // A spread (`.then(...handlers)`) may expand to a rejection handler; treat
    // it as handled rather than risk a false positive.
    true
}

/// Walk up the chain from the outer `.then(...)` call. Returns true if
/// any chained method is `.catch` / `.finally` (which handles rejection)
/// OR the chain is returned / awaited / yielded.
fn chain_is_safe<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current_id = node.id();
    let nodes = semantic.nodes();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::StaticMemberExpression(_) => {
                current_id = parent_id;
                continue;
            }
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee
                    && matches!(member.property.name.as_str(), "catch" | "finally")
                {
                    return true;
                }
                current_id = parent_id;
                continue;
            }
            AstKind::ReturnStatement(_)
            | AstKind::AwaitExpression(_)
            | AstKind::YieldExpression(_) => return true,
            AstKind::ArrowFunctionExpression(a) if a.expression => return true,
            AstKind::VariableDeclarator(_) | AstKind::AssignmentExpression(_) => {
                return true
            }
            AstKind::ExpressionStatement(_) => return false,
            _ => {
                current_id = parent_id;
            }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".then("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "then" {
            return;
        }
        // Already chained from another .then — we're not the outermost.
        // Walk up: if any parent is itself a `.then(...)` call chain
        // we don't want to flag again here.
        let parent_id = semantic.nodes().parent_id(node.id());
        if let AstKind::StaticMemberExpression(_) = semantic.nodes().get_node(parent_id).kind() {
            return;
        }
        // `.then(onFulfilled, onRejected)` handles rejection via its second
        // argument — no trailing `.catch()` is required (Promises/A+).
        if then_has_rejection_handler(call) {
            return;
        }
        if chain_is_safe(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Floating `.then(...)` without a `.catch` / `.finally` and not \
                      returned/awaited — rejection will be swallowed."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_floating_then_without_handler() {
        let d = run_on("p.then((x) => use(x));");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "promise-catch-or-return");
    }

    #[test]
    fn allows_trailing_catch() {
        assert!(run_on("p.then((x) => use(x)).catch((e) => log(e));").is_empty());
    }

    #[test]
    fn allows_two_arg_then_rejection_handler() {
        // #5168: `.then(onFulfilled, onRejected)` handles rejection via the
        // second argument (Promises/A+) — no `.catch()` needed.
        assert!(run_on("p.then((x) => use(x), (e) => log(e));").is_empty());
    }

    #[test]
    fn allows_xstate_two_arg_then() {
        // #5168: XState actors/promise.ts pattern.
        let src = r#"
resolvedPromise.then(
  (response) => {
    if (self.getSnapshot().status !== 'active') return;
    system._relay(self, self, { type: XSTATE_PROMISE_RESOLVE, data: response });
  },
  (errorData) => {
    if (self.getSnapshot().status !== 'active') return;
    system._relay(self, self, { type: XSTATE_PROMISE_REJECT, data: errorData });
  }
);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_then_with_undefined_rejection_handler() {
        // `.then(fn, undefined)` is a no-op rejection handler — still floating.
        let d = run_on("p.then((x) => use(x), undefined);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_then_with_null_rejection_handler() {
        let d = run_on("p.then((x) => use(x), null);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_then_with_void_zero_rejection_handler() {
        // `.then(fn, void 0)` is a no-op rejection handler — still floating.
        let d = run_on("p.then((x) => use(x), void 0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_then_with_spread_second_arg() {
        // `.then(fn, ...rest)` — the spread may expand to a rejection handler,
        // treated as handled to avoid a false positive.
        assert!(run_on("p.then((x) => use(x), ...rest);").is_empty());
    }
}
