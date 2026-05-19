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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
