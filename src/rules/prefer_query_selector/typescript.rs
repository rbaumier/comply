//! prefer-query-selector backend — flag legacy DOM query methods.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[(&str, &str)] = &[
    ("getElementById", "querySelector"),
    ("getElementsByClassName", "querySelectorAll"),
    ("getElementsByTagName", "querySelectorAll"),
    ("getElementsByName", "querySelectorAll"),
];

crate::ast_check! { on ["call_expression"] prefilter = ["getElementById", "getElementsByClassName", "getElementsByTagName", "getElementsByName"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(property) = func.child_by_field_name("property") else { return };
    let method_name = property.utf8_text(source).unwrap_or("");

    let Some((_, replacement)) = METHODS.iter().find(|(m, _)| *m == method_name) else { return };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-query-selector".into(),
        message: format!("Prefer `.{replacement}()` over `.{method_name}()`."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_get_element_by_id() {
        let d = run_on(r#"document.getElementById("foo");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelector"));
    }

    #[test]
    fn flags_get_elements_by_class_name() {
        let d = run_on(r#"document.getElementsByClassName("bar");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelectorAll"));
    }

    #[test]
    fn flags_get_elements_by_tag_name() {
        let d = run_on(r#"document.getElementsByTagName("div");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelectorAll"));
    }

    #[test]
    fn allows_query_selector() {
        assert!(run_on(r##"document.querySelector("#foo");"##).is_empty());
    }
}
