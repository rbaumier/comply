use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    let Some(cond) = node.child_by_field_name("condition") else { return; };
    let cond_text = cond.utf8_text(source).unwrap_or("");
    if !cond_text.contains(".isErr()") {
        return;
    }
    let Some(cons) = node.child_by_field_name("consequence") else { return; };
    let body_text = cons.utf8_text(source).unwrap_or("");
    if !body_text.contains("return Result.err(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Avoid manual error propagation — use Result.gen + yield* instead.".into(),
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
    fn flags_manual_propagation() {
        let src = "function f(r) { if (r.isErr()) { return Result.err(r.error); } return r; }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_yield_propagation() {
        let src = "function f() { return Result.gen(function* () { const v = yield* r; return v; }); }";
        assert!(run(src).is_empty());
    }
}
