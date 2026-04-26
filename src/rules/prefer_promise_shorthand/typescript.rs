//! prefer-promise-shorthand backend — flag `new Promise(resolve => resolve(x))`.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the name of the Nth parameter from a `formal_parameters` node.
fn get_param_name<'a>(
    params: tree_sitter::Node<'a>,
    index: usize,
    source: &'a [u8],
) -> Option<&'a str> {
    let child = params.named_child(index)?;
    match child.kind() {
        "identifier" => child.utf8_text(source).ok(),
        "required_parameter" | "optional_parameter" => child
            .child_by_field_name("pattern")
            .and_then(|p| p.utf8_text(source).ok()),
        _ => None,
    }
}

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    let ctor_name = ctor.utf8_text(source).unwrap_or("");
    if ctor_name != "Promise" { return; }

    // Must have arguments with exactly one argument (the executor).
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 1 { return; }

    let executor = args.named_child(0).unwrap();
    if executor.kind() != "arrow_function" && executor.kind() != "function_expression" {
        return;
    }

    // Get the executor's parameter(s).
    let Some(params) = executor.child_by_field_name("parameters") else { return };
    if params.kind() != "formal_parameters" { return; }

    let first_param = get_param_name(params, 0, source);
    let second_param = get_param_name(params, 1, source);

    if first_param.is_none() { return; }

    // Get the body — must be a single call expression or single-statement block.
    let Some(body) = executor.child_by_field_name("body") else { return };

    let call_node = if body.kind() == "statement_block" {
        if body.named_child_count() != 1 { return; }
        let stmt = body.named_child(0).unwrap();
        match stmt.kind() {
            "expression_statement" => {
                match stmt.named_child(0) {
                    Some(e) if e.kind() == "call_expression" => e,
                    _ => return,
                }
            }
            "return_statement" => {
                match stmt.named_child(0) {
                    Some(e) if e.kind() == "call_expression" => e,
                    _ => return,
                }
            }
            _ => return,
        }
    } else if body.kind() == "call_expression" {
        body
    } else {
        return;
    };

    // The call must be to `resolve(...)` or `reject(...)`.
    let Some(func) = call_node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" { return; }
    let call_name = func.utf8_text(source).unwrap_or("");

    let is_resolve_or_reject = first_param.is_some_and(|p| p == call_name)
        || second_param.is_some_and(|p| p == call_name);

    if !is_resolve_or_reject { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-promise-shorthand".into(),
        message: "`new Promise` wrapping a single resolve/reject — use `Promise.resolve()`/`Promise.reject()` instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_promise_resolve_shorthand() {
        let d = run_on(r#"const p = new Promise((resolve) => resolve(42));"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-promise-shorthand");
    }

    #[test]
    fn flags_promise_reject_shorthand() {
        let d = run_on(r#"const p = new Promise((_, reject) => reject(new Error("fail")));"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_promise_with_logic() {
        let src = "const p = new Promise((resolve, reject) => {\n  fetchData().then(resolve).catch(reject);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_promise_resolve_static() {
        assert!(run_on("const p = Promise.resolve(42);").is_empty());
    }
}
