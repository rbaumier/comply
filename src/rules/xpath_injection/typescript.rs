use crate::diagnostic::{Diagnostic, Severity};

const XPATH_METHODS: &[&str] = &[
    "select",
    "select1",
    "evaluate",
    "selectNodes",
    "selectSingleNode",
];

crate::ast_check! { on ["call_expression"] prefilter = ["evaluate", "selectNodes", "selectSingleNode"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    let method_name = prop.utf8_text(source).unwrap_or("");

    if !XPATH_METHODS.contains(&method_name) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first_arg) = args.named_child(0) else { return; };

    // Flag if first argument (XPath query) is dynamic
    let is_dynamic = match first_arg.kind() {
        "template_string" => {
            let mut cursor = first_arg.walk();
            first_arg.children(&mut cursor).any(|c| c.kind() == "template_substitution")
        }
        "binary_expression" => {
            if let Some(op) = first_arg.child_by_field_name("operator") {
                op.utf8_text(source).unwrap_or("") == "+"
            } else { false }
        }
        "identifier" | "member_expression" => true,
        _ => false,
    };

    if !is_dynamic { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "xpath-injection".into(),
        message: "XPath query with dynamic input — potential XPath injection.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

    #[test]
    fn flags_select_template() {
        assert_eq!(run("xpath.select(`//user[@name='${name}']`, doc)").len(), 1);
    }

    #[test]
    fn flags_select_concat() {
        assert_eq!(run("xpath.select('//user[@id=' + id + ']', doc)").len(), 1);
    }

    #[test]
    fn flags_evaluate_variable() {
        assert_eq!(run("doc.evaluate(query, doc)").len(), 1);
    }

    #[test]
    fn allows_static_xpath() {
        assert!(run("xpath.select('//user', doc)").is_empty());
    }

    #[test]
    fn allows_static_template() {
        assert!(run("xpath.select(`//user[@active='true']`, doc)").is_empty());
    }
}
