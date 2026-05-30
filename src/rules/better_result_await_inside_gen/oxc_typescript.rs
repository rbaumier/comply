//! better-result-await-inside-gen oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Result.gen"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };
        // Walk ancestors to see if we're inside a Result.gen call.
        // Stop at the first Result.gen we find (don't cross into nested ones).
        if !is_inside_result_gen(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inside Result.gen, use `yield* Result.await(...)` instead of `await`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to check if this node is inside a `Result.gen(...)` call.
/// Returns false if:
/// - a nested async non-generator function is found before `Result.gen`
///   (the `await` belongs to that inner async scope, not to the generator)
/// - a nested `Result.gen` is found first (the inner gen has its own scope)
fn is_inside_result_gen<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee
                    && member.property.name.as_str() == "gen"
                    && let Expression::Identifier(obj) = &member.object
                    && obj.name.as_str() == "Result"
                {
                    return true;
                }
            }
            // An async non-generator function creates an independent async scope:
            // `await` inside it is not "inside Result.gen" even if the callback
            // is lexically nested in the generator body.
            AstKind::Function(func) if func.r#async && !func.generator => {
                return false;
            }
            AstKind::ArrowFunctionExpression(arrow) if arrow.r#async => {
                return false;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_oxc_ts;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_direct_await_in_gen() {
        let src = r"
            const r = Result.gen(async function* () {
                const v = await fetch('/');
                return v;
            });
        ";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_yield_await_in_gen() {
        let src = r"
            const r = Result.gen(function* () {
                const v = yield* Result.await(fetch('/'));
                return v;
            });
        ";
        assert!(run(src).is_empty());
    }

    /// Regression for #332 — `await` inside an async callback passed to a
    /// function called from within Result.gen must not be flagged. The async
    /// arrow is a separate async scope; its `await` is not "inside Result.gen".
    #[test]
    fn no_fp_await_in_async_callback_inside_gen() {
        let src = r"
            Result.gen(async function* () {
                const page = yield* Result.await(
                    tryPaginatedQuery(
                        query,
                        async () => countMatching(),
                        async (paginationArgs) => {
                            const rows = await database.findMany(paginationArgs);
                            return rows;
                        },
                    ),
                );
                return Result.ok(page);
            });
        ";
        assert!(run(src).is_empty());
    }

    /// An async function expression (non-arrow) passed as callback must also
    /// be exempt.
    #[test]
    fn no_fp_await_in_async_fn_expression_callback_inside_gen() {
        let src = r"
            Result.gen(async function* () {
                const page = yield* Result.await(
                    fetchPage(async function(args) {
                        const rows = await database.findMany(args);
                        return rows;
                    }),
                );
                return Result.ok(page);
            });
        ";
        assert!(run(src).is_empty());
    }
}
