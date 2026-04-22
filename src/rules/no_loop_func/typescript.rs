//! no-loop-func backend — flag function/arrow/function-expression nodes
//! whose nearest enclosing scope is a loop (`for`, `for..in`, `for..of`,
//! `while`, `do..while`) without passing through another function first.

use crate::diagnostic::{Diagnostic, Severity};

fn is_loop(kind: &str) -> bool {
    matches!(
        kind,
        "for_statement" | "for_in_statement" | "while_statement" | "do_statement"
    )
}

fn is_function(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if !is_function(node.kind()) {
        return;
    }

    // Walk up: bail when we hit another function boundary (nested
    // functions are the user's way to escape loop capture already);
    // flag when we hit a loop first.
    let mut cur = node.parent();
    while let Some(parent) = cur {
        let k = parent.kind();
        if is_function(k) {
            return;
        }
        if is_loop(k) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-loop-func".into(),
                message: "Function declared inside a loop captures loop-scoped bindings — move it out or snapshot the variable.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
        cur = parent.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_function_in_for_loop() {
        assert_eq!(run_on("for (let i = 0; i < 3; i++) { const f = function () { return i; }; }").len(), 1);
    }

    #[test]
    fn flags_arrow_in_while_loop() {
        assert_eq!(run_on("while (cond) { const f = () => i; }").len(), 1);
    }

    #[test]
    fn flags_arrow_in_for_of() {
        assert_eq!(run_on("for (const x of xs) { const f = () => x; }").len(), 1);
    }

    #[test]
    fn allows_function_outside_loop() {
        assert!(run_on("const f = () => 1; for (let i = 0; i < 3; i++) { f(); }").is_empty());
    }

    #[test]
    fn flags_outer_but_not_nested_in_loop() {
        // The outer function is inside the loop — flagged. The inner
        // arrow's nearest function parent is the outer `function`, so
        // it is NOT reported a second time.
        assert_eq!(
            run_on("for (let i = 0; i < 3; i++) { function outer() { const g = () => 1; } }").len(),
            1
        );
    }
}
