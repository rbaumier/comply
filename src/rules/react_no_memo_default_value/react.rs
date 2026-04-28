//! Flags `memo(Component)` / `React.memo(Component)` where the inline component
//! function uses default `[]` or `{}` parameter values. The default reference
//! is recreated on every render, defeating `memo`'s prop-equality short-circuit.

use crate::diagnostic::{Diagnostic, Severity};

fn is_memo_callee(callee: tree_sitter::Node, source: &[u8]) -> bool {
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok() == Some("memo"),
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            obj == Some("React") && prop == Some("memo")
        }
        _ => false,
    }
}

/// Returns the offending default value node (an empty array/object literal),
/// if any, found inside the destructuring patterns of `fn_node`'s parameters.
fn find_unstable_default<'a>(
    fn_node: tree_sitter::Node<'a>,
) -> Option<(tree_sitter::Node<'a>, &'static str)> {
    let params = fn_node.child_by_field_name("parameters")?;
    let mut stack: Vec<tree_sitter::Node> = vec![params];
    while let Some(current) = stack.pop() {
        if matches!(current.kind(), "assignment_pattern" | "object_assignment_pattern")
            && let Some(right) = current.child_by_field_name("right")
        {
            let kind = match right.kind() {
                "array" => "array",
                "object" => "object",
                _ => "",
            };
            if !kind.is_empty() {
                let mut c = right.walk();
                let has_named = right.children(&mut c).any(|ch| ch.is_named());
                if !has_named {
                    return Some((right, kind));
                }
            }
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !is_memo_callee(callee, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(component) = args.children(&mut cursor).find(|c| {
        c.kind() == "arrow_function" || c.kind() == "function_expression"
    }) else {
        return;
    };

    let Some((default_node, kind)) = find_unstable_default(component) else {
        return;
    };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: default_node.start_position().row + 1,
        column: default_node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Default `{kind}` value inside `memo(...)` creates a new reference every render — extract to a module-level constant."
        ),
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
    fn flags_memo_with_array_default() {
        let diags = run(r#"
const List = memo(({ items = [] }) => <ul>{items.map(i => <li>{i}</li>)}</ul>);
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("array"));
    }

    #[test]
    fn flags_memo_with_object_default() {
        let diags = run(r#"
const Card = memo(({ config = {} }) => <div>{config.title}</div>);
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("object"));
    }

    #[test]
    fn flags_react_dot_memo() {
        let diags = run(r#"
const List = React.memo(({ items = [] }) => <ul />);
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_function_expression_inside_memo() {
        let diags = run(r#"
const X = memo(function Inner({ data = {} }) { return <div />; });
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_primitive_default() {
        assert!(run(r#"
const Counter = memo(({ count = 0, label = 'x' }) => <div>{label}: {count}</div>);
"#).is_empty());
    }

    #[test]
    fn allows_no_default() {
        assert!(run(r#"
const List = memo(({ items }) => <ul>{items.map(i => <li>{i}</li>)}</ul>);
"#).is_empty());
    }

    #[test]
    fn allows_non_memo_call() {
        // Only `memo` / `React.memo` is targeted; plain components are
        // covered by a different rule.
        assert!(run(r#"
const List = wrap(({ items = [] }) => <ul />);
"#).is_empty());
    }
}
