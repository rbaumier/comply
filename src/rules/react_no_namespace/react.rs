//! react-no-namespace AST backend.
//!
//! Flags JSX elements or attributes that use XML namespaces (`:`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    // Check element name for namespace.
    if let Some(name_node) = node.child_by_field_name("name")
        && name_node.kind() == "jsx_namespace_name" {
            let Ok(name) = name_node.utf8_text(source) else { return };
            let pos = name_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-namespace".into(),
                message: format!(
                    "Namespaced JSX element `{name}` is not supported by React."
                ),
                severity: Severity::Error,
                span: None,
            });
        }

    // Check attributes for namespace.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        if attr_name.kind() == "jsx_namespace_name" {
            let Ok(name) = attr_name.utf8_text(source) else { continue };
            let pos = attr_name.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-namespace".into(),
                message: format!(
                    "Namespaced JSX attribute `{name}` is not supported by React."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
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
    fn flags_namespaced_element() {
        let src = "const x = <ns:div />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_namespaced_attribute() {
        let src = r#"const x = <div ns:attr="val" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_element() {
        let src = "const x = <div className=\"a\" />;";
        assert!(run(src).is_empty());
    }
}
