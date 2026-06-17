//! Flags `useState(propName)` where the initializer is a destructured prop name.

use crate::diagnostic::{Diagnostic, Severity};

fn collect_destructured_names<'a>(
    pattern: tree_sitter::Node<'a>,
    source: &'a [u8],
    out: &mut Vec<&'a str>,
) {
    match pattern.kind() {
        "object_pattern" => {
            let mut cursor = pattern.walk();
            for child in pattern.children(&mut cursor) {
                if child.kind() == "shorthand_property_identifier_pattern" {
                    if let Ok(name) = child.utf8_text(source) {
                        out.push(name);
                    }
                } else if child.kind() == "pair_pattern" {
                    if let Some(val) = child.child_by_field_name("value") {
                        if let Ok(name) = val.utf8_text(source) {
                            out.push(name);
                        }
                    }
                }
            }
        }
        "identifier" => {
            // Non-destructured: `function App(props)` — we can't track `props.x`.
        }
        _ => {}
    }
}

fn find_object_pattern(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "object_pattern" {
            return Some(current);
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

fn extract_prop_names_from_params<'a>(
    fn_node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Vec<&'a str> {
    let params = match fn_node.child_by_field_name("parameters") {
        Some(p) if p.kind() == "formal_parameters" => p,
        _ => return vec![],
    };
    let Some(first_param) = params.named_child(0) else {
        return vec![];
    };
    let pattern = match find_object_pattern(first_param) {
        Some(p) => p,
        None => return vec![],
    };
    let mut out = vec![];
    collect_destructured_names(pattern, source, &mut out);
    out
}

fn find_component_prop_names<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Vec<&'a str> {
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        match a.kind() {
            "function_declaration" => {
                let name = a
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    ancestor = a.parent();
                    continue;
                }
                return extract_prop_names_from_params(a, source);
            }
            "arrow_function" => {
                let is_component = a
                    .parent()
                    .filter(|p| p.kind() == "variable_declarator")
                    .and_then(|p| p.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_uppercase()));
                if !is_component {
                    ancestor = a.parent();
                    continue;
                }
                return extract_prop_names_from_params(a, source);
            }
            "program" | "class_body" => return vec![],
            _ => {}
        }
        ancestor = a.parent();
    }
    vec![]
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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_use_state_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first_arg) = args.named_child(0) else { return };
    if first_arg.kind() != "identifier" {
        return;
    }
    let Ok(arg_name) = first_arg.utf8_text(source) else { return };

    // `default*` and `initial*` props are initial-value props (controlled/uncontrolled
    // pattern) — they seed state once and are not re-synced, so they are not derived state.
    if arg_name.starts_with("default") || arg_name.starts_with("initial") {
        return;
    }

    let prop_names = find_component_prop_names(node, source);
    if !prop_names.contains(&arg_name) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`useState` initialized from prop `{arg_name}` — derive during render or use `key` prop to reset."
        ),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_use_state_from_destructured_prop() {
        let diags = run(r#"
function UserCard({ name }) {
    const [displayName, setDisplayName] = useState(name);
    return <div>{displayName}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn flags_arrow_component() {
        let diags = run(r#"
const UserCard = ({ value }) => {
    const [val, setVal] = useState(value);
    return <div>{val}</div>;
};
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_pair_pattern_destructuring() {
        let diags = run(r#"
function Card({ initialValue: value }) {
    const [v, setV] = useState(value);
    return <div>{v}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_prop_initializer() {
        assert!(
            run(r#"
function UserCard({ name }) {
    const [count, setCount] = useState(0);
    return <div>{name} {count}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_function_call_initializer() {
        assert!(
            run(r#"
function UserCard({ name }) {
    const [display, setDisplay] = useState(formatName(name));
    return <div>{display}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_non_component_function() {
        assert!(
            run(r#"
function useCustomHook(value) {
    const [v, setV] = useState(value);
    return v;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_undestructured_props() {
        assert!(
            run(r#"
function UserCard(props) {
    const [name, setName] = useState(props.name);
    return <div>{name}</div>;
}
"#)
            .is_empty()
        );
    }

    // Regression test for #483: controlled/uncontrolled pattern with default* props.
    #[test]
    fn allows_default_prefix_prop_as_initial_value() {
        assert!(
            run(r#"
function Sidebar({ defaultOpen, openProp }) {
    const [_open, _setOpen] = React.useState(defaultOpen);
    const open = openProp ?? _open;
    return <div>{open}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_default_value_prop() {
        assert!(
            run(r#"
function Input({ defaultValue }) {
    const [val, setVal] = useState(defaultValue);
    return <input value={val} />;
}
"#)
            .is_empty()
        );
    }

    // Regression test for #3934: `initial*` props are initial-value props, same as `default*`.
    #[test]
    fn allows_initial_prefix_prop_as_initial_value() {
        assert!(
            run(r#"
function DirectionProvider({ initialDirection = 'ltr' }) {
    const [dir, setDir] = useState(initialDirection);
    return <div>{dir}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_initial_value_prop() {
        assert!(
            run(r#"
function Field({ initialValue }) {
    const [value, setValue] = useState(initialValue);
    return <input value={value} />;
}
"#)
            .is_empty()
        );
    }

    // True-positive guard for #3934: a plain prop (not `initial*`/`default*`) still flags.
    #[test]
    fn flags_plain_prop_not_initial_prefixed() {
        let diags = run(r#"
function Card({ value }) {
    const [v, setV] = useState(value);
    return <div>{v}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }
}
