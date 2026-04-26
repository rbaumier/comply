use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let name = prop.utf8_text(source).unwrap_or("");
    if name != "catch" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Replace .catch() on Promise with Result.tryPromise({ try, catch }).".into(),
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
    fn flags_promise_catch_in_better_result_module() {
        let src = "import { Result } from 'better-result';\nfetch('/').catch(e => {});";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_promise_catch_when_no_better_result() {
        let src = "fetch('/').catch(e => {});";
        assert!(run(src).is_empty());
    }
}
