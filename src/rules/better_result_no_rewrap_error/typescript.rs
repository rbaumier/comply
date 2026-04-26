use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "Result.err" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let args_text = args.utf8_text(source).unwrap_or("");
    // Match patterns like `(result.error)` or `(r.error)` — i.e. <ident>.error
    // Strip outer parens
    let inner = args_text.trim().trim_start_matches('(').trim_end_matches(')').trim();
    if !inner.ends_with(".error") {
        return;
    }
    // Must be a plain identifier.error, not a more complex expression
    let base = &inner[..inner.len() - ".error".len()];
    if base.is_empty() || !base.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Avoid re-wrapping error — return `{base}` directly instead of `Result.err({base}.error)`."),
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
    fn flags_rewrap() {
        let src = "function f(result) { return Result.err(result.error); }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_new_error() {
        let src = "function f() { return Result.err(new NotFoundError()); }";
        assert!(run(src).is_empty());
    }
}
