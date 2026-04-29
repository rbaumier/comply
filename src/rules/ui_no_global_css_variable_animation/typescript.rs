//! Flags `document.documentElement.style.setProperty` inside
//! `requestAnimationFrame` or animation-loop callbacks.

use crate::diagnostic::{Diagnostic, Severity};

fn is_document_element_set_property(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    let text = callee.utf8_text(source).ok().unwrap_or("");
    text.contains("document.documentElement.style.setProperty")
        || text.contains("document.documentElement.style.set")
}

fn is_inside_raf(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        if a.kind() == "arrow_function" || a.kind() == "function_expression" {
            if let Some(call) = a.parent().and_then(|args| args.parent()) {
                if call.kind() == "call_expression" {
                    if let Some(callee) = call.child_by_field_name("function") {
                        let name = callee.utf8_text(source).ok().unwrap_or("");
                        if name == "requestAnimationFrame" {
                            return true;
                        }
                    }
                }
            }
        }
        if matches!(
            a.kind(),
            "function_declaration" | "class_declaration" | "method_definition"
        ) {
            return false;
        }
        if a.kind() == "program" {
            break;
        }
        ancestor = a.parent();
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["setProperty"] => |node, source, ctx, diagnostics|
    if !is_document_element_set_property(node, source) {
        return;
    }
    if !is_inside_raf(node, source) {
        return;
    }
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Global CSS variable change inside `requestAnimationFrame` triggers full-page style recalc every frame.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_set_property_in_raf() {
        assert_eq!(run(r#"
requestAnimationFrame(() => {
    document.documentElement.style.setProperty('--scroll', window.scrollY);
});
"#).len(), 1);
    }

    #[test]
    fn allows_scoped_set_property_in_raf() {
        assert!(run(r#"
requestAnimationFrame(() => {
    element.style.setProperty('--x', value);
});
"#).is_empty());
    }

    #[test]
    fn allows_set_property_outside_raf() {
        assert!(run(r#"
document.documentElement.style.setProperty('--theme', 'dark');
"#).is_empty());
    }
}
