//! Detect useEffect that only calls a parent callback to "pass data up".
//!
//! Pattern: `useEffect(() => { onSomething(data) }, [data])`
//! This is an anti-pattern because it creates unnecessary render cycles.
//! The fix is to lift state to the parent.

use crate::diagnostic::{Diagnostic, Severity};

fn is_callback_name(name: &str) -> bool {
    name.starts_with("on")
        && name.len() > 2
        && name.chars().nth(2).is_some_and(|c| c.is_uppercase())
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "useEffect" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(callback) = args.named_child(0) else { return };
    if callback.kind() != "arrow_function" { return; }

    let Some(body) = callback.child_by_field_name("body") else { return };

    // Handle both block body and expression body
    let call_expr = if body.kind() == "statement_block" {
        if body.named_child_count() != 1 { return; }
        let Some(stmt) = body.named_child(0) else { return };
        if stmt.kind() != "expression_statement" { return; }
        stmt.named_child(0)
    } else if body.kind() == "call_expression" {
        Some(body)
    } else {
        return;
    };

    let Some(expr) = call_expr else { return };
    if expr.kind() != "call_expression" { return; }

    let Some(inner_func) = expr.child_by_field_name("function") else { return };
    let func_name = inner_func.utf8_text(source).unwrap_or("");

    // Must be a callback-style name (onXxx)
    if !is_callback_name(func_name) { return; }

    // Skip if it has side effects like fetch
    let call_text = expr.utf8_text(source).unwrap_or("");
    if call_text.contains("await") || call_text.contains("fetch(") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Effect only calls `{func_name}` to pass data to parent — lift state to avoid the extra render cycle."
        ),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_callback_only_effect() {
        assert_eq!(
            run("useEffect(() => { onChange(value) }, [value])").len(),
            1
        );
    }

    #[test]
    fn flags_expression_body() {
        assert_eq!(run("useEffect(() => onUpdate(data), [data])").len(), 1);
    }

    #[test]
    fn flags_on_data_change() {
        assert_eq!(
            run("useEffect(() => { onDataChange(items) }, [items])").len(),
            1
        );
    }

    #[test]
    fn allows_setter_call() {
        // setXxx is handled by react-no-derived-state-in-effect
        assert!(run("useEffect(() => { setData(value) }, [value])").is_empty());
    }

    #[test]
    fn allows_non_callback_name() {
        assert!(run("useEffect(() => { fetchData(id) }, [id])").is_empty());
    }

    #[test]
    fn allows_multi_statement() {
        assert!(run("useEffect(() => { log(x); onChange(x) }, [x])").is_empty());
    }

    #[test]
    fn allows_lowercase_on() {
        // "on" alone or "onclick" (lowercase) is not a callback pattern
        assert!(run("useEffect(() => { on(x) }, [x])").is_empty());
        assert!(run("useEffect(() => { onclick(x) }, [x])").is_empty());
    }

    /// Regression #2253: test-harness components expose state to the outer test via
    /// `useEffect(() => onReady(data), [])`. With `skip_in_test_dir`, the rule must
    /// not fire inside a `.test.tsx` file.
    #[test]
    fn skips_test_harness_files() {
        use crate::rules::test_helpers::run_rule_gated;
        let src = "useEffect(() => { onReady({ setProps, ref }) }, [])";
        assert!(
            run_rule_gated(&Check, src, "tests/mutability.test.tsx").is_empty(),
            "the onReady effect in a .test.tsx fixture must not be flagged"
        );
    }

    /// Negative-space guard: the same pattern in a production `.tsx` file is still
    /// the lift-via-effect anti-pattern and must remain flagged.
    #[test]
    fn still_flags_in_production_files() {
        use crate::rules::test_helpers::run_rule_gated;
        let src = "useEffect(() => { onReady({ setProps, ref }) }, [])";
        assert_eq!(
            run_rule_gated(&Check, src, "src/RigidBody.tsx").len(),
            1,
            "the lift-via-effect pattern must still be flagged in production code"
        );
    }
}
