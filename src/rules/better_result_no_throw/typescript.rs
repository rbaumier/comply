use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

/// True if `node` is contained inside the `try` callback of a `Result.try(...)`
/// or `Result.tryPromise(...)` call — where throwing is the expected migration
/// pattern.
fn inside_result_try_callback(node: Node<'_>, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
        {
            let callee_text = callee.utf8_text(source).unwrap_or("");
            if callee_text == "Result.try" || callee_text == "Result.tryPromise" {
                return true;
            }
        }
        current = n.parent();
    }
    false
}

crate::ast_check! { on ["throw_statement"] prefilter = ["better-result"] => |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    if inside_result_try_callback(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "In modules importing better-result, throw is forbidden — return Result.err(...) instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_throw_in_better_result_module() {
        let src = "import { Result } from 'better-result';\nfunction f() { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_throw_when_no_better_result() {
        let src = "function f() { throw new Error('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_inside_result_try_callback() {
        let src = "import { Result } from 'better-result';\nconst r = Result.try({ try: () => { throw new Error('x'); }, catch: (e) => new MyError({ cause: e }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_inside_result_try_promise_callback() {
        let src = "import { Result } from 'better-result';\nconst r = Result.tryPromise({ try: async () => { throw new Error('x'); }, catch: (e) => new MyError({ cause: e }) });";
        assert!(run(src).is_empty());
    }
}
