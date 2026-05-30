#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::promise_prefer_await_to_then::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_then_inside_async_fn() {
        let diags = run("async function f() { fetchUser(id).then(d => d); }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_then_outside_async_fn() {
        assert!(run("fetchUser(id).then(d => d);").is_empty());
    }

    #[test]
    fn allows_zod_then_combinator_inside_async_fn() {
        // Regression for #115 — Zod `.then(transform)` is a schema combinator.
        let diags = run(
            "async function f() { const s = z.string().then(v => v.trim()); return s; }",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_zod_catch_chain_inside_async_fn() {
        let diags = run(
            "async function f() { const s = z.coerce.number().int().min(1).catch(1); return s; }",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_react_lazy_import_reshaping_inside_async_fn() {
        // Regression for #427 — React.lazy() requires a sync callback; the inner
        // .then() cannot be replaced with await even when inside an async function.
        let diags = run(
            r#"import { lazy } from "react";
async function setup() {
    const Dialog = lazy(() =>
      import("@/features/dialog").then((module) => ({ default: module.Dialog }))
    );
}"#,
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }
}
