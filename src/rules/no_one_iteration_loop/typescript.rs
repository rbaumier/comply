//! no-one-iteration-loop backend.
//!
//! Flag a loop whose body unconditionally terminates on the first iteration.
//! A loop terminates unconditionally when every control-flow path through
//! its body hits a `return`, `break`, or `throw` before reaching the end —
//! in this checker we approximate that with the simpler "the last
//! statement in the body is always-terminating and no earlier branch
//! could keep the loop going".
//!
//! To stay precise and avoid false positives we require the body to be a
//! `statement_block` (braced loops only) and the FINAL statement to be a
//! `return`, `break`, or `throw` that is NOT nested inside a conditional.

use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

crate::ast_check! { on ["for_statement", "for_in_statement", "while_statement", "do_statement"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    // Find the last named statement; if it's an unconditional exit AND
    // no earlier statement is a `continue`, the loop runs exactly once.
    let mut cursor = body.walk();
    let stmts: Vec<_> = body.named_children(&mut cursor).collect();
    let Some(last) = stmts.last() else { return };

    if !is_unconditional_exit(*last) {
        return;
    }

    // If any earlier statement contains a `continue`, the loop may
    // iterate more than once. Bail out conservatively.
    for s in &stmts[..stmts.len().saturating_sub(1)] {
        if contains_continue(*s) {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-one-iteration-loop".into(),
        message: "Loop body always exits on the first iteration — the loop is redundant.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn is_unconditional_exit(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "return_statement" | "break_statement" | "throw_statement"
    )
}

fn contains_continue(node: tree_sitter::Node) -> bool {
    if node.kind() == "continue_statement" {
        return true;
    }
    // Don't descend into nested loops — their `continue` belongs to them.
    if LOOP_KINDS.contains(&node.kind()) {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_continue(child) {
            return true;
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_for_with_unconditional_return() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        doWork();
        return;
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_while_with_unconditional_break() {
        let src = r#"function f() {
    while (true) {
        doWork();
        break;
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_for_in_with_unconditional_throw() {
        let src = r#"function f(obj: Record<string, unknown>) {
    for (const k in obj) {
        throw new Error(k);
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_loop_with_conditional_break() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        if (cond(i)) break;
        doWork(i);
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_loop_with_continue() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        if (i === 0) continue;
        return;
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_normal_loop() {
        let src = r#"function f() {
    for (let i = 0; i < 10; i++) {
        doWork(i);
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
