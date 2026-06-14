#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::invariant_requires_message::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_gated(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    #[test]
    fn flags_invariant_without_message() {
        let diags = run("invariant(router != null);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_invariant_with_message() {
        assert!(run("invariant(router != null, \"Router must be initialized\");").is_empty());
    }

    #[test]
    fn allows_invariant_with_template_literal() {
        assert!(run("invariant(x > 0, `Expected positive, got ${x}`);").is_empty());
    }

    #[test]
    fn ignores_method_call() {
        assert!(run("const obj = { invariant() {} }; obj.invariant(x);").is_empty());
    }

    #[test]
    fn ignores_other_functions() {
        assert!(run("assert(x > 0);").is_empty());
    }

    #[test]
    fn allows_invariant_with_nested_call() {
        assert!(run("invariant(arr.includes(x), \"missing\");").is_empty());
    }

    #[test]
    fn flags_invariant_with_nested_call_no_message() {
        let diags = run("invariant(arr.includes(x));");
        assert_eq!(diags.len(), 1);
    }

    // Regression #2220: in test files, messageless `invariant(cond)` is an
    // intentional type-narrowing assertion (the runner reports file+line), so
    // the central `skip_in_test_dir` gate suppresses the rule there.
    #[test]
    fn skips_messageless_invariant_in_test_file() {
        assert!(run_gated("invariant(x instanceof Comment);", "src/frame.test.tsx").is_empty());
        assert!(run_gated("invariant(node != null);", "src/router.spec.ts").is_empty());
    }

    // Negative-space guard: messageless `invariant()` in production source is a
    // genuine warning and must still fire.
    #[test]
    fn flags_messageless_invariant_in_production_file() {
        let diags = run_gated("invariant(router != null);", "src/router.ts");
        assert_eq!(diags.len(), 1);
    }
}
