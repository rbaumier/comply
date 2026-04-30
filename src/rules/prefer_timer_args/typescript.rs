use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["setTimeout", "setInterval"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    let func_name = func.utf8_text(source).unwrap_or("");

    if func_name != "setTimeout" && func_name != "setInterval" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first_arg) = args.named_child(0) else { return; };

    // Check if first argument is an arrow function that just calls another function
    if first_arg.kind() != "arrow_function" { return; }

    let Some(body) = first_arg.child_by_field_name("body") else { return; };

    // Arrow with expression body: () => fn(args)
    if body.kind() == "call_expression" {
        // Check it's a simple function call (not method call or complex expression)
        if let Some(callee) = body.child_by_field_name("function")
            && callee.kind() == "identifier" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-timer-args".into(),
                    message: format!("Pass arguments directly to `{func_name}` instead of wrapping in arrow function."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

    #[test]
    fn flags_arrow_wrapper() {
        assert_eq!(run("setTimeout(() => doSomething(arg), 100)").len(), 1);
    }

    #[test]
    fn flags_set_interval() {
        assert_eq!(run("setInterval(() => tick(count), 1000)").len(), 1);
    }

    #[test]
    fn allows_direct_args() {
        assert!(run("setTimeout(doSomething, 100, arg)").is_empty());
    }

    #[test]
    fn allows_method_call() {
        // Method calls can't use the direct args pattern
        assert!(run("setTimeout(() => obj.method(arg), 100)").is_empty());
    }

    #[test]
    fn allows_complex_body() {
        assert!(run("setTimeout(() => { doA(); doB(); }, 100)").is_empty());
    }
}
