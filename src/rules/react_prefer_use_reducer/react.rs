//! Counts top-level `useState` calls in a PascalCase component body and flags
//! components with 4+ of them.

use crate::diagnostic::{Diagnostic, Severity};

const USE_STATE_THRESHOLD: usize = 4;

fn component_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "function_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = name_node.utf8_text(source).ok()?;
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(name)
            } else {
                None
            }
        }
        "arrow_function" => {
            let parent = node.parent()?;
            if parent.kind() != "variable_declarator" {
                return None;
            }
            let name_node = parent.child_by_field_name("name")?;
            let name = name_node.utf8_text(source).ok()?;
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(name)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_use_state_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok() == Some("useState"),
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            obj == Some("React") && prop == Some("useState")
        }
        _ => false,
    }
}

/// Counts `useState` calls reachable from `node` without crossing a nested
/// function boundary — only state declared in the component body itself.
fn count_use_state(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    if is_use_state_call(node, source) {
        count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "arrow_function" | "function_expression" | "function_declaration" => continue,
            _ => count += count_use_state(child, source),
        }
    }
    count
}

crate::ast_check! { on ["function_declaration", "arrow_function"] => |node, source, ctx, diagnostics|
    let Some(name) = component_name(node, source) else { return };

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    let count = count_use_state(body, source);

    if count < USE_STATE_THRESHOLD {
        return;
    }

    let report_node = if node.kind() == "arrow_function" {
        node.parent().unwrap_or(node)
    } else {
        node
    };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: report_node.start_position().row + 1,
        column: report_node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Component `{name}` has {count} `useState` calls — consider `useReducer` for related state."
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
    fn flags_four_use_state() {
        let diags = run(r#"
function Form() {
    const [a, setA] = useState('');
    const [b, setB] = useState('');
    const [c, setC] = useState('');
    const [d, setD] = useState('');
    return <div />;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Form"));
        assert!(diags[0].message.contains("4"));
    }

    #[test]
    fn flags_arrow_component() {
        let diags = run(r#"
const Wizard = () => {
    const [a, setA] = useState(0);
    const [b, setB] = useState(0);
    const [c, setC] = useState(0);
    const [d, setD] = useState(0);
    const [e, setE] = useState(0);
    return <div />;
};
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Wizard"));
    }

    #[test]
    fn flags_react_dot_use_state() {
        let diags = run(r#"
function Panel() {
    const [a] = React.useState();
    const [b] = React.useState();
    const [c] = React.useState();
    const [d] = React.useState();
    return <div />;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_three_use_state() {
        assert!(run(r#"
function Form() {
    const [a, setA] = useState('');
    const [b, setB] = useState('');
    const [c, setC] = useState('');
    return <div />;
}
"#).is_empty());
    }

    #[test]
    fn allows_lowercase_function() {
        // Custom hook, not a component.
        assert!(run(r#"
function useForm() {
    const [a, setA] = useState('');
    const [b, setB] = useState('');
    const [c, setC] = useState('');
    const [d, setD] = useState('');
    return { a, b, c, d };
}
"#).is_empty());
    }

    #[test]
    fn ignores_use_state_in_nested_function() {
        // useState inside a nested helper isn't part of the component body.
        assert!(run(r#"
function App() {
    const [only] = useState(0);
    function inner() {
        const [a] = useState();
        const [b] = useState();
        const [c] = useState();
    }
    return <div />;
}
"#).is_empty());
    }
}
