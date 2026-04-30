//! Flags `useMemo(() => expr, [...])` where `expr` is trivially cheap.

use crate::diagnostic::{Diagnostic, Severity};

fn is_simple_expression(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" | "number" | "string" | "true" | "false" | "null" => true,
        "template_string" => {
            let mut cursor = node.walk();
            node.children(&mut cursor).all(|child| {
                if child.kind() == "template_substitution" {
                    child
                        .named_child(0)
                        .is_none_or(|expr| is_simple_expression(expr, source))
                } else {
                    true
                }
            })
        }
        "binary_expression" => {
            let l = node.child_by_field_name("left");
            let r = node.child_by_field_name("right");
            l.is_some_and(|n| is_simple_expression(n, source))
                && r.is_some_and(|n| is_simple_expression(n, source))
        }
        "unary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if op == "delete" || op == "void" {
                return false;
            }
            node.child_by_field_name("argument")
                .is_some_and(|n| is_simple_expression(n, source))
        }
        "member_expression" => {
            let computed = node.child_by_field_name("index").is_some();
            if computed {
                return false;
            }
            node.child_by_field_name("object")
                .is_some_and(|n| is_simple_expression(n, source))
        }
        "ternary_expression" => {
            let cond = node.child_by_field_name("condition");
            let cons = node.child_by_field_name("consequence");
            let alt = node.child_by_field_name("alternative");
            cond.is_some_and(|n| is_simple_expression(n, source))
                && cons.is_some_and(|n| is_simple_expression(n, source))
                && alt.is_some_and(|n| is_simple_expression(n, source))
        }
        "parenthesized_expression"
        | "as_expression"
        | "non_null_expression"
        | "satisfies_expression" => node
            .named_child(0)
            .is_some_and(|c| is_simple_expression(c, source)),
        _ => false,
    }
}

fn is_usememo_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok() == Some("useMemo"),
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            obj == Some("React") && prop == Some("useMemo")
        }
        _ => false,
    }
}

fn get_return_expression(callback: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let body = callback.child_by_field_name("body")?;
    if body.kind() != "statement_block" {
        return Some(body);
    }
    let mut cursor = body.walk();
    let named: Vec<_> = body
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    if named.len() != 1 {
        return None;
    }
    let stmt = named[0];
    if stmt.kind() != "return_statement" {
        return None;
    }
    let mut sc = stmt.walk();
    stmt.children(&mut sc).find(|c| c.is_named())
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_usememo_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(callback) = args.children(&mut cursor).find(|c| {
        c.kind() == "arrow_function" || c.kind() == "function_expression"
    }) else {
        return;
    };

    let Some(ret_expr) = get_return_expression(callback) else { return };
    if !is_simple_expression(ret_expr, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`useMemo` wrapping a trivially cheap expression — memo overhead exceeds the computation.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_simple_identifier() {
        assert_eq!(run("const x = useMemo(() => value, [value]);").len(), 1);
    }

    #[test]
    fn flags_simple_binary() {
        assert_eq!(run("const x = useMemo(() => a + b, [a, b]);").len(), 1);
    }

    #[test]
    fn flags_simple_member_access() {
        assert_eq!(run("const x = useMemo(() => user.name, [user]);").len(), 1);
    }

    #[test]
    fn flags_simple_ternary() {
        assert_eq!(
            run("const x = useMemo(() => a ? b : c, [a, b, c]);").len(),
            1
        );
    }

    #[test]
    fn flags_block_body_with_return() {
        assert_eq!(
            run("const x = useMemo(() => { return a + b; }, [a, b]);").len(),
            1
        );
    }

    #[test]
    fn flags_react_dot_usememo() {
        assert_eq!(
            run("const x = React.useMemo(() => a + b, [a, b]);").len(),
            1
        );
    }

    #[test]
    fn allows_function_call() {
        assert!(run("const x = useMemo(() => computeExpensiveValue(a, b), [a, b]);").is_empty());
    }

    #[test]
    fn allows_object_literal() {
        assert!(run("const x = useMemo(() => ({ key: value }), [value]);").is_empty());
    }

    #[test]
    fn allows_array_literal() {
        assert!(run("const x = useMemo(() => [a, b, c], [a, b, c]);").is_empty());
    }

    #[test]
    fn allows_multi_statement_body() {
        assert!(
            run("const x = useMemo(() => { const tmp = a + b; return tmp * 2; }, [a, b]);")
                .is_empty()
        );
    }

    #[test]
    fn allows_computed_member() {
        assert!(run("const x = useMemo(() => arr[index], [arr, index]);").is_empty());
    }

    #[test]
    fn allows_string_literal_not_usememo() {
        assert!(run("const x = useCallback(() => value, [value]);").is_empty());
    }

    #[test]
    fn flags_chained_member_access() {
        assert_eq!(
            run("const x = useMemo(() => user.address.city, [user]);").len(),
            1
        );
    }

    #[test]
    fn flags_unary_negation() {
        assert_eq!(run("const x = useMemo(() => -count, [count]);").len(), 1);
    }

    #[test]
    fn flags_parenthesized() {
        assert_eq!(run("const x = useMemo(() => (a + b), [a, b]);").len(), 1);
    }

    #[test]
    fn flags_as_expression() {
        assert_eq!(
            run("const x = useMemo(() => value as Foo, [value]);").len(),
            1
        );
    }

    #[test]
    fn allows_template_with_fn_call() {
        assert!(run("const x = useMemo(() => `${expensive()}`, []);").is_empty());
    }

    #[test]
    fn flags_template_with_simple_substitution() {
        assert_eq!(
            run("const x = useMemo(() => `hello ${name}`, [name]);").len(),
            1
        );
    }

    #[test]
    fn allows_no_callback_arg() {
        assert!(run("const x = useMemo(computeFn, [dep]);").is_empty());
    }

    #[test]
    fn allows_delete_in_unary() {
        assert!(run("const x = useMemo(() => delete obj.key, [obj]);").is_empty());
    }
}
