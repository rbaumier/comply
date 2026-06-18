//! Integration tests for ts-prefer-promise-with-resolvers via the registered rule.

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_rule_by_id;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_rule_by_id("ts-prefer-promise-with-resolvers", source, "t.ts")
    }

    // A self-contained executor that only calls `resolve`/`reject` is the
    // idiomatic constructor use; `withResolvers` would only make it longer.
    // (This shape was flagged by the old TextCheck — the encoded false positive.)
    #[test]
    fn allows_self_contained_executor() {
        let src = "const p = new Promise((resolve, reject) => resolve(1));";
        assert!(run(src).is_empty());
    }

    // Two self-contained executors, neither leaking — both left alone.
    #[test]
    fn allows_multiple_self_contained_executors() {
        let src = "const a = new Promise((r) => r(1));\nconst b = new Promise((r) => r(2));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_with_resolvers() {
        let src = "const { promise, resolve } = Promise.withResolvers();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_resolve_static() {
        let src = "const p = Promise.resolve(42);";
        assert!(run(src).is_empty());
    }

    // The settle handle is stored in an outer binding so it can be called from
    // outside the executor — exactly the case `withResolvers` exists for.
    #[test]
    fn flags_escaping_executor() {
        let src = "let r; const p = new Promise((resolve) => { r = resolve; }); r();";
        assert_eq!(run(src).len(), 1);
    }
}
