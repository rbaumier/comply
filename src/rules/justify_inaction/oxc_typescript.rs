use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TryStatement,
            AstType::IfStatement,
            AstType::SwitchCase,
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
                AstKind::SwitchCase(case) => {
                    // Only flag default case (test is None)
                    if case.test.is_none() && case.consequent.is_empty() {
                        // Check if there's a comment within the case span
                        let span_text = &ctx.source[case.span.start as usize..case.span.end as usize];
                        if !span_text.contains("//") && !span_text.contains("/*") {
                            flag(ctx, case.span.start, "default", diagnostics);
                        }
                    }
                }
                AstKind::WhileStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
                            flag(ctx, stmt.span.start, "while", diagnostics);
                        }
                }
                AstKind::DoWhileStatement(stmt) => {
                    if let Statement::BlockStatement(block) = &stmt.body
                        && block_is_empty_no_comment(&block.body, ctx.source, block.span) {
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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

    // ── loops ────────────────────────────────────────────────────

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("while (poll()) {}").len(), 1);
    }

    #[test]
    fn flags_empty_do_while() {
        assert_eq!(run_on("do {} while (cond());").len(), 1);
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
        let src = "while (poll()) { /* busy wait for the device */ }";
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
