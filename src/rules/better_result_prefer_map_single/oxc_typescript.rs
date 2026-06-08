use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Walk up the ancestors of `node` (a `Result.gen(...)` CallExpression)
/// past any type wrappers / parens / `await` and return the closest
/// enclosing call expression that *contains* this one as an argument.
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

/// Count `yield*` expressions within a function body by scanning the source
/// text range for the function argument. We use the semantic nodes to find
/// `YieldExpression` nodes whose `delegate` flag is true.
fn count_yield_stars_in_range<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    start: u32,
    end: u32,
) -> usize {
    let mut count = 0;
    for node in semantic.nodes().iter() {
        let AstKind::YieldExpression(yield_expr) = node.kind() else {
            continue;
        };
        if yield_expr.delegate
            && yield_expr.span.start >= start
            && yield_expr.span.end <= end
        {
            count += 1;
        }
    }
    count
}

/// Return true if the generator body contains any `if` statement — a sign
/// that the logic after the single `yield*` is conditional and cannot be
/// expressed as a plain `.map()` / `.andThen()` chain.
fn has_if_in_range<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    start: u32,
    end: u32,
) -> bool {
    for node in semantic.nodes().iter() {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            continue;
        };
        if if_stmt.span.start >= start && if_stmt.span.end <= end {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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
        // Check callee is `Result.gen`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Result" || member.property.name.as_str() != "gen" {
            return;
        }
        // Find the generator function argument
        let Some(gen_arg) = call.arguments.iter().find(|arg| {
            matches!(
                arg,
                Argument::FunctionExpression(_)
            )
        }) else {
            return;
        };
        let Argument::FunctionExpression(func) = gen_arg else {
            return;
        };
        if !func.generator {
            return;
        }
        let yields = count_yield_stars_in_range(semantic, func.span.start, func.span.end);
        if yields != 1 {
            return;
        }
        // Conditional logic after the yield* (if/else branches) means the
        // generator cannot be mechanically replaced with .map()/.andThen().
        if has_if_in_range(semantic, func.span.start, func.span.end) {
            return;
        }

        // Skip when the `Result.gen(...)` is the direct argument of
        // `unwrapOrThrow(...)`. That's the canonical Elysia handler
        // shape — every route runs through `unwrapOrThrow(Result.gen(...))`
        // for uniformity, even when the body has a single yield*.
        // Mixing `.map()` handlers and `Result.gen` handlers across a
        // codebase erases that uniformity for no real win.
        if let Some(parent_call) = nearest_enclosing_call(node, semantic)
            && let Expression::Identifier(callee_id) = &parent_call.callee
            && callee_id.name.as_str() == "unwrapOrThrow"
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Result.gen wrapping a single yield* — use .map()/.andThen() instead.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_naked_single_yield_result_gen() {
        let src = r#"
            const r = Result.gen(function* () {
                const x = yield* fetchUser();
                return Result.ok(x);
            });
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_unwrap_or_throw_wrapper() {
        // Regression for rbaumier/comply#34 — the canonical Elysia
        // handler shape unwrapOrThrow(Result.gen(...)) is mandated by
        // the project's CLAUDE.md for uniformity.
        let src = r#"
            const handler = async ({ session }) =>
                unwrapOrThrow(Result.gen(async function* () {
                    yield* scopeFilterOrganizations(session);
                    return tryPaginatedQuery();
                }));
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_single_yield_with_conditional_after() {
        // Regression for rbaumier/comply#539 — a single yield* followed by
        // conditional logic (if/return) cannot be expressed as plain
        // .map()/.andThen() and must not be flagged.
        let src = r#"
            return Result.gen(async function* () {
                const row = yield* Result.await(tryDatabaseQuery(() => db.query.user.findFirst({})));
                if (row === undefined) {
                    return internalError('user disappeared');
                }
                return dispatchSessionScope({ ...row });
            });
        "#;
        assert!(run(src).is_empty());
    }
}
