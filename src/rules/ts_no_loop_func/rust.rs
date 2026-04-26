//! no-loop-func (Rust) — flag closures declared inside loop bodies.
//!
//! In Rust the capture is governed by the borrow checker, so the
//! runtime-footgun angle is weaker than in JS. The rule still warns
//! because repeatedly allocating a closure inside a loop body is usually
//! a sign the author meant to hoist it.

use crate::diagnostic::{Diagnostic, Severity};

fn is_loop_rust(kind: &str) -> bool {
    matches!(
        kind,
        "for_expression" | "while_expression" | "loop_expression"
    )
}

fn is_function_rust(kind: &str) -> bool {
    matches!(
        kind,
        "closure_expression" | "function_item"
    )
}

crate::ast_check! { on ["closure_expression", "function_item"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut cur = node.parent();
    while let Some(parent) = cur {
        let k = parent.kind();
        if is_function_rust(k) {
            return;
        }
        if is_loop_rust(k) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-loop-func".into(),
                message: "Closure declared inside a loop body — hoist it out so it is not rebuilt per iteration.".into(),
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_closure_in_for_loop() {
        let src = "fn main() { for i in 0..3 { let f = |x: i32| x + i; let _ = f(1); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_closure_in_while_loop() {
        let src = "fn main() { while cond() { let f = || 1; let _ = f(); } } fn cond() -> bool { true }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_closure_outside_loop() {
        let src = "fn main() { let f = |x: i32| x + 1; for _ in 0..3 { let _ = f(1); } }";
        assert!(run_on(src).is_empty());
    }
}
