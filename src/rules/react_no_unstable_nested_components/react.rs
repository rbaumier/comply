//! react-no-unstable-nested-components AST backend.
//!
//! Detects component definitions nested inside other component functions.
//! React will see a new component type on every render and destroy the
//! entire subtree's DOM and state.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true when the node subtree contains any JSX.
fn subtree_has_jsx(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => true,
        _ => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|child| subtree_has_jsx(child))
        }
    }
}

/// Return true if a function node has a PascalCase name.
fn is_component_name(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "function_declaration" => {
            let Some(name) = node.child_by_field_name("name") else {
                return false;
            };
            let Ok(t) = name.utf8_text(source) else {
                return false;
            };
            t.starts_with(|c: char| c.is_ascii_uppercase())
        }
        "arrow_function" => {
            let Some(parent) = node.parent() else {
                return false;
            };
            if parent.kind() != "variable_declarator" {
                return false;
            }
            let Some(name) = parent.child_by_field_name("name") else {
                return false;
            };
            let Ok(t) = name.utf8_text(source) else {
                return false;
            };
            t.starts_with(|c: char| c.is_ascii_uppercase())
        }
        _ => false,
    }
}

crate::ast_check! { on ["function_declaration", "arrow_function"] => |node, source, ctx, diagnostics|
    // Match function_declaration or arrow_function that looks like a component.
    let is_arrow = node.kind() == "arrow_function";

    if !is_component_name(node, source) {
        return;
    }

    if !subtree_has_jsx(node) {
        return;
    }

    // Check if this component is nested inside another component function.
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        match a.kind() {
            "function_declaration" | "arrow_function" => {
                if is_component_name(a, source) && subtree_has_jsx(a) {
                    // This is a nested component inside another component.
                    let pos = if is_arrow {
                        // For arrow functions, point to the variable_declarator
                        node.parent().unwrap_or(node).start_position()
                    } else {
                        node.start_position()
                    };
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "react-no-unstable-nested-components".into(),
                        message: "Do not define components during render. React will \
                                  see a new component type on every render and destroy \
                                  the entire subtree's DOM and state. Move it outside \
                                  the parent component."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
            // Stop at class or module level.
            "class_body" | "program" => return,
            _ => {}
        }
        ancestor = a.parent();
    }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_nested_arrow_component() {
        let src = r#"
function ParentComponent() {
    const NestedComponent = () => {
        return <div>nested</div>;
    };
    return <NestedComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_function_component() {
        let src = r#"
function ParentComponent() {
    function ChildComponent() {
        return <span>child</span>;
    }
    return <ChildComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_top_level_component() {
        let src = r#"
function MyComponent() {
    return <div>hello</div>;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component_nested_function() {
        let src = r#"
function ParentComponent() {
    const handleClick = () => {
        console.log("clicked");
    };
    return <button onClick={handleClick}>click</button>;
}
"#;
        assert!(run(src).is_empty());
    }
}
