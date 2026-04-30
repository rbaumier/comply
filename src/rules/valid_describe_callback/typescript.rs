//! valid-describe-callback — describe callback must be sync, parameter-less, non-returning.

use crate::diagnostic::{Diagnostic, Severity};

/// Check whether the call expression's callee is `describe` (bare) or
/// `describe.skip` / `describe.only` / `describe.each(...)` / similar.
fn is_describe_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or("") == "describe",
        "member_expression" => callee
            .child_by_field_name("object")
            .and_then(|o| o.utf8_text(source).ok())
            .map(|t| t == "describe")
            .unwrap_or(false),
        "call_expression" => is_describe_call(callee, source),
        _ => false,
    }
}

/// Return true if the function node is async. Covers `arrow_function`,
/// `function_expression`, and `function_declaration`.
fn is_async_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
        // Some grammar versions expose `async` as an unnamed leaf whose text is "async".
        if !child.is_named() && child.utf8_text(source).unwrap_or("") == "async" {
            return true;
        }
    }
    false
}

/// Return true if the function node declares any parameters.
fn has_parameters(node: tree_sitter::Node) -> bool {
    // Arrow functions may use a single identifier as the parameter (no
    // formal_parameters wrapper). Handle both shapes.
    if node.kind() == "arrow_function"
        && let Some(param) = node.child_by_field_name("parameter")
        && param.kind() == "identifier"
    {
        return true;
    }
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params.children(&mut cursor).any(|c| c.is_named())
}

/// Walk the function body looking for a `return` statement with an argument,
/// without descending into nested functions.
fn body_returns_value(body: tree_sitter::Node) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if matches!(
            child.kind(),
            "function_expression" | "function_declaration" | "arrow_function" | "method_definition"
        ) {
            continue;
        }
        if child.kind() == "return_statement" {
            // return_statement has an optional expression child. If it has a
            // named child, a value is returned.
            let mut c = child.walk();
            if child.children(&mut c).any(|n| n.is_named()) {
                return true;
            }
        }
        if body_returns_value(child) {
            return true;
        }
    }
    false
}

/// Inspect the callback passed as the second argument to a describe call and
/// push a diagnostic if it violates the rule.
fn check_callback(
    call: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(args) = call.child_by_field_name("arguments") else {
        return;
    };
    // Collect named children of the arguments node.
    let mut cursor = args.walk();
    let named: Vec<_> = args
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    let Some(cb) = named.get(1) else { return };

    let is_fn = matches!(
        cb.kind(),
        "arrow_function" | "function_expression" | "function_declaration"
    );
    if !is_fn {
        return;
    }

    let async_flag = is_async_fn(*cb, source);
    let params_flag = has_parameters(*cb);
    let return_flag = match cb.child_by_field_name("body") {
        // Arrow with expression body (implicit return): any non-empty body
        // returns a value.
        Some(body) if body.kind() != "statement_block" => true,
        Some(body) => body_returns_value(body),
        None => false,
    };

    let message = if async_flag {
        "`describe` callback must not be async."
    } else if params_flag {
        "`describe` callback must not declare parameters."
    } else if return_flag {
        "`describe` callback must not return a value."
    } else {
        return;
    };

    let pos = cb.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "valid-describe-callback".into(),
        message: message.into(),
        severity: Severity::Warning,
        span: Some((cb.byte_range().start, cb.byte_range().len())),
    });
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_describe_call(node, source) {
        return;
    }
    check_callback(node, source, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_ts(source, &Check)
    }

    #[test]
    fn flags_async_arrow_callback() {
        let d = run("describe('suite', async () => { it('x', () => {}); });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "valid-describe-callback");
        assert!(d[0].message.contains("async"));
    }

    #[test]
    fn flags_async_function_expression() {
        let d = run("describe('suite', async function () { it('x', () => {}); });");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async"));
    }

    #[test]
    fn flags_callback_with_parameters() {
        let d = run("describe('suite', (done) => { it('x', () => {}); });");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("parameters"));
    }

    #[test]
    fn flags_callback_returning_value() {
        let src = "describe('suite', () => { it('x', () => {}); return 42; });";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }

    #[test]
    fn flags_arrow_with_implicit_return() {
        let d = run("describe('suite', () => 42);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }

    #[test]
    fn allows_valid_sync_callback() {
        let d = run("describe('suite', () => { it('x', () => {}); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_sync_function_expression() {
        let d = run("describe('suite', function () { it('x', () => {}); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_bare_return_without_value() {
        let d = run("describe('suite', () => { if (skip) return; it('x', () => {}); });");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_return_inside_nested_function() {
        let src = "describe('suite', () => { it('x', () => { return 1; }); });";
        let d = run(src);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_describe_only_with_async_callback() {
        let d = run("describe.only('suite', async () => {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async"));
    }
}
