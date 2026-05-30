#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::async_await_only::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_then_chain() {
        let diags = run("fetchUser(id).then(data => { console.log(data); });");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".then()"));
    }

    #[test]
    fn flags_catch_chain() {
        let diags = run("fetchUser(id).catch(err => { console.error(err); });");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".catch()"));
    }

    #[test]
    fn flags_then_and_catch() {
        let diags = run("fetchUser(id).then(d => d).catch(e => e);");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_await() {
        assert!(run("async function f() { const d = await fetchUser(id); }").is_empty());
    }

    #[test]
    fn allows_array_then() {
        assert!(run("const arr = [1, 2]; arr.map(x => x);").is_empty());
    }

    #[test]
    fn allows_zod_catch_fallback() {
        // Regression for #115 — Zod `.catch(fallback)` is a schema combinator.
        let diags = run("const schema = z.coerce.number().int().min(1).catch(1);");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_zod_then_combinator() {
        let diags = run("const schema = z.string().then(v => v.trim());");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_react_lazy_import_reshaping() {
        // Regression for #427 — React.lazy() requires a sync callback returning a
        // Promise; .then() reshapes the module and cannot be replaced with await.
        let diags = run(
            r#"import { lazy } from "react";
const Dialog = lazy(() =>
  import("@/features/dialog").then((module) => ({ default: module.Dialog }))
);"#,
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_react_lazy_with_member_callee() {
        // Regression for #427 — React.lazy(...) member-expression form.
        let diags = run(
            r#"import React from "react";
const Dialog = React.lazy(() =>
  import("@/features/dialog").then((module) => ({ default: module.Dialog }))
);"#,
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }
}
