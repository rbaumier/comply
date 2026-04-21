use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.utf8_text(source).unwrap_or("") != "useEffect" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let callback = match args.named_child(0) {
        Some(c) => c,
        None => return,
    };
    if callback.kind() != "arrow_function" { return; }

    let body = match callback.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };
    if body.kind() != "statement_block" { return; }

    // Body must have exactly one named statement
    if body.named_child_count() != 1 { return; }
    let stmt = match body.named_child(0) {
        Some(s) => s,
        None => return,
    };
    if stmt.kind() != "expression_statement" { return; }
    let expr = match stmt.named_child(0) {
        Some(e) => e,
        None => return,
    };
    if expr.kind() != "call_expression" { return; }

    let call_text = expr.utf8_text(source).unwrap_or("");
    if call_text.contains("await") || call_text.contains("fetch(")
        || call_text.contains("subscribe(") || call_text.contains("addEventListener(")
    {
        return;
    }

    let inner_func = match expr.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if !inner_func.utf8_text(source).unwrap_or("").starts_with("set") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Derived state in `useEffect` is an anti-pattern. Compute the value during render instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_setter_only_effect() {
        assert_eq!(
            run("useEffect(() => { setFull(first + ' ' + last) }, [first, last])").len(),
            1
        );
    }

    #[test]
    fn allows_effect_with_fetch() {
        assert!(run("useEffect(() => { fetch('/api').then(setData) }, [id])").is_empty());
    }

    #[test]
    fn allows_multi_statement_effect() {
        assert!(
            run("useEffect(() => { const x = a + b; setFull(x); log(x) }, [a, b])").is_empty()
        );
    }

    #[test]
    fn allows_non_setter_call() {
        assert!(run("useEffect(() => { cleanup() }, [])").is_empty());
    }
}
