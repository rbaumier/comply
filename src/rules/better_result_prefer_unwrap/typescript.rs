use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    if node.child_by_field_name("alternative").is_some() {
        return;
    }
    let Some(cond) = node.child_by_field_name("condition") else { return; };
    let cond_text = cond.utf8_text(source).unwrap_or("");
    if !cond_text.contains(".isErr()") {
        return;
    }
    let Some(cons) = node.child_by_field_name("consequence") else { return; };
    let body_text = cons.utf8_text(source).unwrap_or("").trim();
    let Some(var_name) = cond_text.split(".isErr()").next().and_then(|s| {
        let t = s.trim().trim_start_matches('(');
        if t.is_empty() { None } else { Some(t) }
    }) else { return; };
    let throw_pattern = format!("throw {}.error", var_name);
    if !body_text.contains(&throw_pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Use `{}.unwrap()` instead of manually checking `.isErr()` and throwing.",
            var_name,
        ),
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
    fn flags_iserr_throw() {
        let src = "const r = doSomething();\nif (r.isErr()) {\n  throw r.error;\n}";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_unwrap() {
        let src = "const r = doSomething().unwrap();";
        assert!(run(src).is_empty());
    }
}
