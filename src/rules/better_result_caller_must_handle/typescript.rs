use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

/// Heuristic: consider the call a Result-producer if its callee name matches
/// one of the well-known `Result.*` factories, ends with "Result" (e.g.
/// `findUserResult`), or starts with a common Result-returning prefix
/// (`try`, `attempt`, `safe`).
///
/// Limitation: without TypeScript type information we can't detect every
/// function that returns `Result<T, E>` — only those whose name follows a
/// recognisable convention. Callers that don't follow the convention are not
/// flagged.
fn returns_result(callee_text: &str) -> bool {
    if matches!(
        callee_text,
        "Result.ok" | "Result.err" | "Result.try" | "Result.tryPromise" | "Result.gen"
    ) {
        return true;
    }
    let last_segment = callee_text.rsplit('.').next().unwrap_or(callee_text);
    if last_segment.ends_with("Result") {
        return true;
    }
    starts_with_camel_prefix(last_segment, "try")
        || starts_with_camel_prefix(last_segment, "attempt")
        || starts_with_camel_prefix(last_segment, "safe")
}

/// `tryFoo`, `attemptBar`, `safeBaz` — prefix followed by an uppercase letter.
fn starts_with_camel_prefix(name: &str, prefix: &str) -> bool {
    name.len() > prefix.len()
        && name.starts_with(prefix)
        && name
            .as_bytes()
            .get(prefix.len())
            .is_some_and(|b| b.is_ascii_uppercase())
}

crate::ast_check! { on ["expression_statement"] prefilter = ["better-result"] => |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
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
    #[test]
    fn flags_try_prefixed_call() {
        let src = "import { Result } from 'better-result';\ntryFetchUser(id);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_attempt_prefixed_call() {
        let src = "import { Result } from 'better-result';\nattemptParse(input);";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_safe_prefixed_call() {
        let src = "import { Result } from 'better-result';\nsafeDivide(a, b);";
        assert_eq!(run(src).len(), 1);
    }
    /// Documents the heuristic limitation: a function returning `Result<T, E>`
    /// whose name doesn't follow the `Result` / `try*` / `attempt*` / `safe*`
    /// convention is *not* flagged. Detecting it would require type info.
    #[test]
    fn limitation_does_not_flag_arbitrary_returning_call() {
        let src = "import { Result } from 'better-result';\nfindUser(id);";
        assert!(run(src).is_empty());
    }
}
