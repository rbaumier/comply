use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    if node.kind() != "throw_statement" {
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
}
