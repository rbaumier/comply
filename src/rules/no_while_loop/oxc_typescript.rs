use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// A loop condition that always evaluates truthy (`while (true)`, `while (1)`).
///
/// Such a loop is an intentionally unbounded event/dispatch loop that terminates
/// via interior `break`/`return`/`throw` on a runtime event — it has no counter
/// and no collection to iterate, so it is not mechanically convertible to a
/// `for`/`for-of` and must not be flagged.
fn is_constant_truthy(test: &Expression) -> bool {
    match test {
        Expression::BooleanLiteral(lit) => lit.value,
        Expression::NumericLiteral(lit) => lit.value != 0.0,
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::WhileStatement, AstType::DoWhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (loop_type, span) = match node.kind() {
            AstKind::WhileStatement(stmt) => {
                if is_constant_truthy(&stmt.test) {
                    return;
                }
                ("while", stmt.span)
            }
            AstKind::DoWhileStatement(stmt) => {
                if is_constant_truthy(&stmt.test) {
                    return;
                }
                ("do-while", stmt.span)
            }
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-while-loop".into(),
            message: format!("`{loop_type}` loop — prefer recursion or higher-order functions."),
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

    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_while_with_counter_condition() {
        assert_eq!(run("while (i < n) { process(i); i++; }").len(), 1);
    }

    #[test]
    fn flags_while_with_node_condition() {
        assert_eq!(run("while (node) { node = node.next; }").len(), 1);
    }

    #[test]
    fn flags_while_with_queue_condition() {
        assert_eq!(run("while (queue.length) { queue.pop(); }").len(), 1);
    }

    #[test]
    fn flags_do_while() {
        assert_eq!(run("do { x++; } while (x < 10);").len(), 1);
    }

    #[test]
    fn allows_constant_truthy_dispatch_loop() {
        // Streaming bytecode/opcode dispatch: terminates on an interior break on
        // the end-opcode, not a counter — not convertible to for/for-of.
        let code = "while (true) { const op = readByte(); if (op === END) break; dispatch(op); }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_while_one() {
        assert!(run("while (1) { if (done()) break; }").is_empty());
    }

    #[test]
    fn allows_do_while_constant_truthy() {
        assert!(run("do { if (done()) break; } while (true);").is_empty());
    }

    #[test]
    fn allows_for_of() {
        assert!(run("for (const x of items) { process(x); }").is_empty());
    }

    #[test]
    fn allows_map() {
        assert!(run("items.map(x => x * 2);").is_empty());
    }
}
