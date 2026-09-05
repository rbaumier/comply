use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TryStatement,
            AstType::IfStatement,
            AstType::SwitchStatement,
            AstType::WhileStatement,
            AstType::DoWhileStatement,
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::ForOfStatement,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
            match node.kind() {
                AstKind::TryStatement(try_stmt) => {
                    // Check catch clause
                    if let Some(handler) = &try_stmt.handler
                        && block_is_empty_no_comment(&handler.body.body, ctx.source, handler.body.span) {
                            flag(ctx, handler.span.start, "catch", diagnostics);
                        }
                    // Check finally clause (finalizer is a BlockStatement)
                    if let Some(finalizer) = &try_stmt.finalizer
                        && block_is_empty_no_comment(&finalizer.body, ctx.source, finalizer.span) {
                            flag(ctx, finalizer.span.start, "finally", diagnostics);
                        }
                }
                AstKind::IfStatement(stmt) => {
                    // Check if consequence is empty block
                    if let Statement::BlockStatement(block) = &stmt.consequent
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, stmt.span.start, "if", diagnostics);
                        }
                    // Check else branch (alternate)
                    if let Some(Statement::BlockStatement(block)) = &stmt.alternate
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, block.span.start, "else", diagnostics);
                        }
                }
                AstKind::SwitchStatement(switch) => {
                    let cases = &switch.cases;
                    for (i, case) in cases.iter().enumerate() {
                        // Only an empty `default:` (no test, no consequent) needs justifying.
                        if case.test.is_some() || !case.consequent.is_empty() {
                            continue;
                        }
                        // OXC ends an empty case's span right after its `:`, so a
                        // justifying comment lands *outside* the case span. Scan the
                        // default's body region instead: from after the `:` to the start
                        // of the next case, or the switch's closing `}`. That covers an
                        // inline `default: // fall through` and a comment on a following
                        // line, while excluding any comment after the closing brace.
                        let region_end = cases
                            .get(i + 1)
                            .map_or(switch.span.end as usize, |next| next.span.start as usize);
                        let region = &ctx.source[case.span.end as usize..region_end];
                        if !region.contains("//") && !region.contains("/*") {
                            flag(ctx, case.span.start, "default", diagnostics);
                        }
                    }
                }
                AstKind::WhileStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span)
                        && !condition_contains_call(&stmt.test, semantic) {
                            flag(ctx, stmt.span.start, "while", diagnostics);
                        }
                }
                AstKind::DoWhileStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span)
                        && !condition_contains_call(&stmt.test, semantic) {
                            flag(ctx, stmt.span.start, "do-while", diagnostics);
                        }
                }
                AstKind::ForStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, stmt.span.start, "for", diagnostics);
                        }
                }
                AstKind::ForInStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, stmt.span.start, "for-in", diagnostics);
                        }
                }
                AstKind::ForOfStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, stmt.span.start, "for-of", diagnostics);
                        }
                }
                _ => {}
            }
    }
}

/// True when the loop condition contains a call — `while (queue.shift()) {}`,
/// `while ((m = re.exec(s)) !== null) {}`, `while (await next()) {}`.
///
/// A call there is what advances the loop, so the whole iteration is the
/// condition and the empty body is the drain/poll idiom rather than forgotten
/// work; a body comment could only restate the condition. A condition with no
/// call — a bare flag (`while (running) {}`), a literal, a pure comparison — is
/// not self-documenting and is still flagged. The Rust backend draws the same
/// line on the same idiom.
///
/// Answered by span containment over the semantic nodes: any call nested
/// anywhere in the condition counts, whatever wraps it.
fn condition_contains_call(condition: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let condition_span = condition.span();
    semantic.nodes().iter().any(|node| {
        let AstKind::CallExpression(call) = node.kind() else {
            return false;
        };
        condition_span.start <= call.span.start && call.span.end <= condition_span.end
    })
}

/// Returns true if the block body has no statements AND the source text
/// within the span contains no comments.
fn block_is_empty_no_comment(stmts: &[Statement], source: &str, span: oxc_span::Span) -> bool {
    if !stmts.is_empty() {
        return false;
    }
    let start = span.start as usize;
    let end = span.end as usize;
    if end > source.len() {
        return true;
    }
    let text = &source[start..end];
    !text.contains("//") && !text.contains("/*")
}

fn flag(ctx: &CheckCtx, offset: u32, what: &str, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Empty `{what}` block \u{2014} add a comment inside explaining why the inaction is intentional."
        ),
        severity: super::META.severity,
        span: None,
    });
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

    // ── catch / finally ──────────────────────────────────────────

    #[test]
    fn flags_empty_catch() {
        assert_eq!(run_on("try { x(); } catch (e) {}").len(), 1);
    }

    #[test]
    fn allows_catch_with_comment_inside() {
        assert!(run_on("try { x(); } catch (e) { /* swallowed intentionally */ }").is_empty());
    }

    #[test]
    fn flags_empty_finally() {
        assert_eq!(run_on("try { x(); } finally {}").len(), 1);
    }

    // ── if / else ────────────────────────────────────────────────

    #[test]
    fn flags_empty_if() {
        assert_eq!(run_on("if (x) {}").len(), 1);
    }

    #[test]
    fn flags_empty_else() {
        assert_eq!(run_on("if (x) { a(); } else {}").len(), 1);
    }

    #[test]
    fn allows_else_with_comment_inside() {
        let src = "if (x) { a(); } else { /* no-op by design */ }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_else_if_chain() {
        assert!(run_on("if (x === 1) { a(); } else if (x === 2) { b(); }").is_empty());
    }

    // ── switch default ───────────────────────────────────────────

    #[test]
    fn flags_empty_default() {
        let src = "switch (x) { case 1: a(); break; default: }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_default_with_inline_comment() {
        // #6184: comment on the same line as `default:` justifies the empty case.
        let src = "switch (x) { case 1: a(); break; default: // fall through\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_default_with_comment_on_following_line() {
        let src = "switch (x) {\n  case 1: a(); break;\n  default:\n    // intentional no-op\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_default_with_comment_only_after_closing_brace() {
        // Negative space: a comment after the switch's `}` is outside the
        // default's region, so the empty default is still flagged.
        let src = "switch (x) { case 1: a(); break; default: } // handle later";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_non_terminal_default_with_inline_comment() {
        // `default` followed by another label: region runs to the next case.
        let src = "switch (x) { default: // fall through\n case 1: a(); break; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_terminal_default_without_comment() {
        let src = "switch (x) { default: case 1: a(); break; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // ── loops ────────────────────────────────────────────────────

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("while (running) {}").len(), 1);
    }

    #[test]
    fn flags_empty_do_while() {
        assert_eq!(run_on("do {} while (i-- > 0);").len(), 1);
    }

    // ── call-in-condition exemption (issue #1436) ────────────────

    #[test]
    fn allows_empty_while_draining_a_queue_issue_1436() {
        // The call in the condition is what advances the loop — the drain idiom,
        // the JS twin of the Rust register-polling loop the Rust backend exempts.
        assert!(run_on("while (queue.shift()) {}").is_empty());
    }

    #[test]
    fn allows_empty_while_negated_call_issue_1436() {
        assert!(run_on("while (!device.isReady()) {}").is_empty());
    }

    #[test]
    fn allows_empty_while_assignment_of_call_issue_1436() {
        // `while ((m = re.exec(s)) !== null) {}` — the call is nested two levels
        // down in the condition and still drives every iteration.
        assert!(run_on("while ((m = re.exec(s)) !== null) {}").is_empty());
    }

    #[test]
    fn allows_empty_while_awaited_call_issue_1436() {
        let src = "async function f() { while (await next()) {} }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_do_while_call_issue_1436() {
        assert!(run_on("do {} while (queue.shift());").is_empty());
    }

    #[test]
    fn flags_empty_while_comparison_no_call_issue_1436() {
        // Narrowness guard: no call, so the condition explains nothing about why
        // the body is empty.
        assert_eq!(run_on("while (x < n) {}").len(), 1);
    }

    #[test]
    fn flags_empty_while_literal_no_call_issue_1436() {
        assert_eq!(run_on("while (true) {}").len(), 1);
    }

    #[test]
    fn flags_empty_for_with_call_in_its_test_issue_1436() {
        // Narrowness guard: the exemption is for condition-driven loops only — a
        // `for` loop carries its own update slot, so an empty body there is not
        // the drain idiom even when the test calls something.
        assert_eq!(run_on("for (let i = 0; hasMore(i); i++) {}").len(), 1);
    }

    #[test]
    fn flags_empty_for() {
        assert_eq!(run_on("for (let i = 0; i < 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_empty_for_of() {
        assert_eq!(run_on("for (const x of xs) {}").len(), 1);
    }

    #[test]
    fn allows_busy_wait_with_comment() {
        // A call-free condition, so the in-body comment is what clears it.
        let src = "while (running) { /* busy wait for the device */ }";
        assert!(run_on(src).is_empty());
    }

    // ── scope exclusions ─────────────────────────────────────────

    #[test]
    fn does_not_flag_empty_function_body() {
        assert!(run_on("function stub() {}").is_empty());
    }

    #[test]
    fn does_not_flag_empty_arrow_body() {
        assert!(run_on("const noop = () => {};").is_empty());
    }

    #[test]
    fn does_not_flag_empty_method_body() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }
}
