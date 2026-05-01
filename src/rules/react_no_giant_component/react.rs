//! Flags React components whose body exceeds 300 lines.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["function_declaration", "arrow_function"] => |node, source, ctx, diagnostics|
    let Some(name) = component_name(node, source) else { return };
    if !subtree_has_jsx(node) {
        return;
    }

    let max = ctx.config.threshold("react-no-giant-component", "max", ctx.lang);

    let start_line = node.start_position().row;
    let end_line = node.end_position().row;
    let line_count = end_line - start_line + 1;

    if line_count <= max {
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
            "Component `{name}` is {line_count} lines — break into smaller focused components."
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

    fn make_giant_component(name: &str, lines: usize) -> String {
        let mut s = format!("function {name}() {{\n");
        for i in 0..lines.saturating_sub(3) {
            s.push_str(&format!("    const x{i} = {i};\n"));
        }
        s.push_str("    return <div>big</div>;\n}\n");
        s
    }

    #[test]
    fn flags_giant_function_component() {
        let src = make_giant_component("HugeComponent", 350);
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("HugeComponent"));
        assert!(diags[0].message.contains("350"));
    }

    #[test]
    fn flags_giant_arrow_component() {
        let mut s = "const HugeArrow = () => {\n".to_string();
        for i in 0..348 {
            s.push_str(&format!("    const x{i} = {i};\n"));
        }
        s.push_str("    return <div>big</div>;\n};\n");
        let diags = run(&s);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("HugeArrow"));
    }

    #[test]
    fn allows_small_component() {
        assert!(
            run(r#"
function SmallComponent() {
    return <div>hello</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_giant_non_component_function() {
        let mut s = "function processData() {\n".to_string();
        for i in 0..350 {
            s.push_str(&format!("    const x{i} = {i};\n"));
        }
        s.push_str("    return 42;\n}\n");
        assert!(run(&s).is_empty());
    }

    #[test]
    fn allows_at_threshold() {
        let src = make_giant_component("ExactComponent", 300);
        assert!(run(&src).is_empty());
    }
}
