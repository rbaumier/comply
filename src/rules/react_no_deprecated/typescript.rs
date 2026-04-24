//! react-no-deprecated backend — flag deprecated React / ReactDOM member
//! accesses and legacy lifecycle method definitions inside class bodies.

use crate::diagnostic::{Diagnostic, Severity};

/// Pairs of `(object, property)` that identify a deprecated React or
/// ReactDOM API. `ReactDOM.findDOMNode` is intentionally excluded — it is
/// handled by the dedicated `react-no-find-dom-node` rule.
const DEPRECATED_REACT_MEMBERS: &[(&[u8], &[u8])] = &[
    (b"React", b"createClass"),
    (b"React", b"PropTypes"),
    (b"React", b"DOM"),
    (b"ReactDOM", b"render"),
    (b"ReactDOM", b"hydrate"),
    (b"ReactDOM", b"unmountComponentAtNode"),
];

const DEPRECATED_LIFECYCLES: &[&[u8]] = &[
    b"componentWillMount",
    b"componentWillReceiveProps",
    b"componentWillUpdate",
];

/// Returns true if any ancestor of `node` is a class body (i.e. the method
/// is defined inside a `class_declaration` or `class_expression`). This
/// avoids flagging object literals that happen to use the same name.
fn is_inside_class(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "class_body" | "class_declaration" | "class_expression" => return true,
            _ => {}
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "member_expression" => {
            let Some(object) = node.child_by_field_name("object") else { return };
            let Some(property) = node.child_by_field_name("property") else { return };
            if object.kind() != "identifier" {
                return;
            }
            let obj_bytes = &source[object.byte_range()];
            let prop_bytes = &source[property.byte_range()];
            let Some((obj, prop)) = DEPRECATED_REACT_MEMBERS
                .iter()
                .find(|(o, p)| *o == obj_bytes && *p == prop_bytes)
            else {
                return;
            };
            let pos = node.start_position();
            let obj_str = std::str::from_utf8(obj).unwrap_or("?");
            let prop_str = std::str::from_utf8(prop).unwrap_or("?");
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-deprecated".into(),
                message: format!(
                    "`{obj_str}.{prop_str}` is deprecated. Replace it with its modern equivalent."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        "method_definition" => {
            let Some(name) = node.child_by_field_name("name") else { return };
            let name_bytes = &source[name.byte_range()];
            if !DEPRECATED_LIFECYCLES.contains(&name_bytes) {
                return;
            }
            if !is_inside_class(node) {
                return;
            }
            let pos = node.start_position();
            let name_str = std::str::from_utf8(name_bytes).unwrap_or("?");
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-deprecated".into(),
                message: format!(
                    "`{name_str}` is deprecated. Use the modern lifecycle (e.g. \
                     `componentDidMount`, `getDerivedStateFromProps`, \
                     `getSnapshotBeforeUpdate`) or prefix with `UNSAFE_`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_react_create_class() {
        let d = run_on("React.createClass({});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reactdom_render() {
        let d = run_on("ReactDOM.render(<App />, root);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_component_will_mount() {
        let src = "class App extends React.Component {\n  componentWillMount() {}\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_component_did_mount() {
        let src = "class App extends React.Component {\n  componentDidMount() {}\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unsafe_prefix() {
        let src = "class App extends React.Component {\n  UNSAFE_componentWillMount() {}\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_react_prop_types() {
        let d = run_on("const types = React.PropTypes;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reactdom_hydrate() {
        let d = run_on("ReactDOM.hydrate(<App />, root);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_create_root() {
        assert!(run_on("ReactDOM.createRoot(root).render(<App />);").is_empty());
    }

    #[test]
    fn flags_reactdom_unmount() {
        let d = run_on("ReactDOM.unmountComponentAtNode(root);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_lifecycle_outside_class() {
        let src = "const obj = { componentWillMount() {} };";
        assert!(run_on(src).is_empty());
    }
}
