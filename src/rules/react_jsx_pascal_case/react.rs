//! react-jsx-pascal-case AST backend.
//!
//! Flags JSX components whose name is not PascalCase.
//! HTML intrinsic elements (all-lowercase) are ignored.

use crate::diagnostic::{Diagnostic, Severity};

fn is_pascal_case(name: &str) -> bool {
    // Allow namespaced (Foo.Bar) — check each segment.
    for segment in name.split('.') {
        if segment.is_empty() {
            return false;
        }
        let first = segment.chars().next().unwrap();
        // Must start with uppercase.
        if !first.is_ascii_uppercase() {
            return false;
        }
        // Must not contain underscores or hyphens (SCREAMING_CASE, kebab).
        if segment.contains('_') || segment.contains('-') {
            return false;
        }
    }
    true
}

fn is_intrinsic(name: &str) -> bool {
    // HTML/SVG intrinsic elements are all-lowercase or contain hyphens (web components).
    let first = name.chars().next().unwrap_or('a');
    first.is_ascii_lowercase()
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    // Skip intrinsic HTML elements.
    if is_intrinsic(tag) {
        return;
    }

    if !is_pascal_case(tag) {
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-pascal-case".into(),
            message: format!(
                "Component `{tag}` is not PascalCase — rename to PascalCase."
            ),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_non_pascal_case_component() {
        let src = "const x = <MY_COMPONENT />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pascal_case() {
        let src = "const x = <MyComponent />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_namespaced_pascal() {
        let src = "const x = <Foo.Bar />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_html_elements() {
        let src = "const x = <div>hello</div>;";
        assert!(run(src).is_empty());
    }
}
