//! react-display-name backend — detect `export default` of an anonymous
//! arrow function / function expression that renders JSX, including when
//! wrapped in `React.memo(...)` or `React.forwardRef(...)`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `node` contains a JSX element anywhere in its subtree.
fn contains_jsx(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => return true,
            _ => {}
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `node` is an anonymous function expression / arrow function
/// (no `name` field) that produces JSX.
fn is_anonymous_component(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "arrow_function" => contains_jsx(node),
        "function_expression" | "function" => {
            if node.child_by_field_name("name").is_some() {
                return false;
            }
            contains_jsx(node)
        }
        _ => false,
    }
}

/// True if `call` is `React.memo(...)` or `React.forwardRef(...)`, or the
/// bare `memo(...)` / `forwardRef(...)` forms.
fn is_react_wrapper_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else { return false };
    match func.kind() {
        "identifier" => {
            let name = &source[func.byte_range()];
            name == b"memo" || name == b"forwardRef"
        }
        "member_expression" => {
            let Some(object) = func.child_by_field_name("object") else { return false };
            let Some(property) = func.child_by_field_name("property") else { return false };
            if &source[object.byte_range()] != b"React" {
                return false;
            }
            let prop = &source[property.byte_range()];
            prop == b"memo" || prop == b"forwardRef"
        }
        _ => false,
    }
}

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    // Only default exports: `export default <value>`.
    let mut cursor = node.walk();
    let mut has_default = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "default" {
            has_default = true;
            break;
        }
    }
    if !has_default {
        return;
    }

    // Find the exported value among named children.
    let mut exported: Option<tree_sitter::Node> = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        exported = Some(child);
    }
    let Some(value) = exported else { return };

    let anonymous = if is_anonymous_component(value) {
        Some(value)
    } else if value.kind() == "call_expression" && is_react_wrapper_call(value, source) {
        let args = value.child_by_field_name("arguments");
        let mut found: Option<tree_sitter::Node> = None;
        if let Some(args) = args {
            let mut c = args.walk();
            for a in args.named_children(&mut c) {
                if is_anonymous_component(a) {
                    found = Some(a);
                    break;
                }
            }
        }
        found
    } else {
        None
    };

    let Some(target) = anonymous else { return };
    let pos = target.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-display-name".into(),
        message: "Anonymous React component missing a display name.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_anonymous_arrow_default_export() {
        let d = run_on("export default () => <div />;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_anonymous_memo_default_export() {
        let d = run_on("export default React.memo(() => <div />);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_anonymous_forward_ref_default_export() {
        let d = run_on("export default React.forwardRef((props, ref) => <div ref={ref} />);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_memo_default_export() {
        let d = run_on("export default memo(() => <div />);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_function_default_export() {
        assert!(run_on("export default function MyComponent() { return <div />; }").is_empty());
    }

    #[test]
    fn allows_named_arrow_then_export() {
        let src = "const Foo = () => <div />;\nexport default Foo;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_memo_with_named_component() {
        let src = "const Foo = () => <div />;\nexport default React.memo(Foo);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_jsx_default_export() {
        assert!(run_on("export default () => 42;").is_empty());
    }
}
