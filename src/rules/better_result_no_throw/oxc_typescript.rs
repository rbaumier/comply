//! OxcCheck backend — flag `throw` in modules importing better-result.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };
        if !ctx.source_contains("better-result") && !ctx.source_contains("@better-result") {
            return;
        }
        if inside_result_try_callback(node, semantic) {
            return;
        }
        // Typed-throw bridge: `throw X.error` re-throws a Result's
        // already-typed ApiError so the framework's error middleware
        // can map it to a Problem response. This is the canonical
        // `unwrapOrThrow(promise)` shape mandated by Amadeo's CLAUDE.md
        // and used by every Elysia handler. The throw IS the helper's
        // contract, not an escape hatch.
        if let Expression::StaticMemberExpression(member) = &throw.argument
            && member.property.name.as_str() == "error"
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "In modules importing better-result, throw is forbidden \u{2014} return Result.err(...) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to check if this node is inside a context where throwing is
/// the expected pattern:
/// - `Result.try(...)` / `Result.tryPromise(...)` — static constructors
/// - `result.match({ ok: ..., err: ... })` — instance combinator whose `err`
///   callback may need to throw to satisfy a third-party API contract
///   (e.g. Better Auth hooks that require throwing APIError).
fn inside_result_try_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind()
            && let Expression::StaticMemberExpression(member) = &call.callee {
                let prop = member.property.name.as_str();
                if (prop == "try" || prop == "tryPromise")
                    && let Expression::Identifier(obj) = &member.object
                        && obj.name.as_str() == "Result" {
                            return true;
                        }
                if prop == "match" {
                    return true;
                }
            }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_bare_throw_in_better_result_module() {
        let src = r#"
            import { Result } from "better-result";
            function f() { throw new Error("oops"); }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_throw_inside_result_try() {
        let src = r#"
            import { Result } from "better-result";
            const r = Result.try(() => { throw new Error("oops"); });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_x_error_bridge() {
        // Regression for rbaumier/comply#40 — `throw result.error` is
        // the canonical Result→typed-throw bridge used by unwrapOrThrow.
        let src = r#"
            import { Result } from "better-result";
            async function unwrapOrThrow<T, E>(p: Promise<Result<T, E>>): Promise<T> {
                const result = await p;
                if (result.isErr()) {
                    throw result.error;
                }
                return result.value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_inside_match_err_callback() {
        // Regression for #540 — Better Auth hooks require throwing APIError
        // inside the `.match()` err callback; Result-based return is impossible.
        let src = r#"
            import { Result } from "better-result";
            scopeResult.match({
              ok: (scope) => ({ data: { ...session, ...scope } }),
              err: (apiError) => {
                throw new APIError(
                  apiError.status === 403 ? 'FORBIDDEN' : 'INTERNAL_SERVER_ERROR',
                  { ...apiError }
                );
              },
            });
        "#;
        assert!(run(src).is_empty());
    }
}
