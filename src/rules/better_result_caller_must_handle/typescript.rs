use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

/// Heuristic: consider the call a Result-producer if its callee name ends with
/// "Result" (e.g. `findUserResult`) or is `Result.ok`, `Result.err`, `Result.try`,
/// `Result.tryPromise`, `Result.gen`.
fn returns_result(callee_text: &str) -> bool {
    matches!(
        callee_text,
        "Result.ok" | "Result.err" | "Result.try" | "Result.tryPromise" | "Result.gen"
    ) || callee_text.ends_with("Result")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    if node.kind() != "expression_statement" {
        return;
    }
    let mut cursor = node.walk();
    let Some(inner) = node.children(&mut cursor).find(|c| c.kind() == "call_expression") else {
        return;
    };
    let Some(callee) = inner.child_by_field_name("function") else { return; };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !returns_result(callee_text) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Returned Result from `{callee_text}(...)` is ignored — assign, match, map, unwrap, or yield* it."),
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
    fn flags_ignored_result_call() {
        let src = "import { Result } from 'better-result';\nfindUserResult(id);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_assigned_result() {
        let src = "import { Result } from 'better-result';\nconst r = findUserResult(id);";
        assert!(run(src).is_empty());
    }
}
