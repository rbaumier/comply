//! better-result-caller-must-handle OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Callee names whose argument is treated as a handled Result.
const TERMINAL_SINK_CALLEES: &[&str] = &["unwrapOrThrow"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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
        if !super::imports_better_result(ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !super::returns_result(callee_text) {
            return;
        }

        if let Some(parent_call) = nearest_enclosing_call(node, semantic)
            && let Expression::Identifier(callee_id) = &parent_call.callee
            && TERMINAL_SINK_CALLEES.contains(&callee_id.name.as_str())
        {
            return;
        }

        // Only flag if the call is an expression statement (result is ignored).
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }

        // The concise body of an arrow (`(tx) => Result.gen(...)`) is the arrow's
        // return value, which oxc models as a synthetic ExpressionStatement. A
        // returned Result is handled by the arrow's caller, exactly as a block-body
        // `return Result.gen(...)` is — so this is not an ignored result.
        if is_concise_arrow_body(parent.id(), semantic.nodes()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Returned Result from `{callee_text}(...)` is ignored \u{2014} assign, match, map, unwrap, or yield* it."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// `true` when `expr_stmt_id` is the synthetic `ExpressionStatement` oxc emits
/// for the concise body of an arrow function (`(x) => expr`): that statement's
/// parent is a `FunctionBody` whose parent is an `ArrowFunctionExpression` with
/// `expression == true`.
fn is_concise_arrow_body(
    expr_stmt_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let body = nodes.parent_node(expr_stmt_id);
    if !matches!(body.kind(), AstKind::FunctionBody(_)) {
        return false;
    }
    matches!(
        nodes.parent_node(body.id()).kind(),
        AstKind::ArrowFunctionExpression(arrow) if arrow.expression
    )
}

/// Returns the immediately-enclosing call expression, transparent to parens, await, and TS type wrappers. None if no enclosing call exists.
fn nearest_enclosing_call<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a oxc_ast::ast::CallExpression<'a>> {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(_)
            | AstKind::AwaitExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSTypeAssertion(_)
            | AstKind::TSNonNullExpression(_) => {
                current_id = parent_id;
            }
            AstKind::CallExpression(call) => return Some(call),
            _ => return None,
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
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_ignored_result_gen_statement() {
        let src = "import { Result } from 'better-result';\nResult.gen(function* () { return Result.ok(1); });\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn accepts_unwrap_or_throw_wrapping_result_gen_in_handler() {
        let src = r#"
            import { Result, unwrapOrThrow } from 'better-result';
            import { Elysia } from 'elysia';

            new Elysia().post("/things", ({ body }) =>
                unwrapOrThrow(
                    Result.gen(async function* () {
                        const row = yield* Result.await(tryDatabaseQuery(() => db.insert(thing).values(body).returning()));
                        return Result.ok(firstOrError(row, "INSERT RETURNING yielded no row"));
                    }),
                ),
            );
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn accepts_unwrap_or_throw_wrapping_result_gen_statement() {
        let src = r#"
            import { Result, unwrapOrThrow } from 'better-result';
            unwrapOrThrow(Result.gen(function* () { return Result.ok(1); }));
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for #4071 — a `Result.gen(...)` that is the concise body of an
    // arrow callback is the arrow's return value (consumed by the higher-order
    // caller and awaited), not an ignored result.
    #[test]
    fn accepts_result_gen_returned_from_concise_arrow_callback() {
        let src = r#"
            import { Result } from 'better-result';

            const deactivated = yield* Result.await(
                transactionalQuery(database, (tx) =>
                    Result.gen(async function* () {
                        const n = yield* Result.await(tryDatabaseQuery(() => tx.$count(team, where)));
                        if (n > 0) return Result.err(new ConflictError({}));
                        return Result.ok(row);
                    }),
                ),
            );
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Discriminator — a block-body arrow that drops the Result in statement
    // position still flags; the exemption covers concise bodies (returns) only.
    #[test]
    fn flags_result_gen_dropped_in_block_body_arrow() {
        let src = r#"
            import { Result } from 'better-result';
            transactionalQuery(database, (tx) => {
                Result.gen(function* () { return Result.ok(1); });
            });
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
