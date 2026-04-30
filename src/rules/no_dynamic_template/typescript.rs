//! no-dynamic-template backend — flag dynamic HTML construction APIs.

use crate::diagnostic::{Diagnostic, Severity};

const ASSIGNMENT_PROPS: &[&str] = &["innerHTML", "outerHTML"];

const CALL_METHODS: &[&str] = &[
    "document.write",
    "document.writeln",
    "insertAdjacentHTML",
    "createContextualFragment",
    "setHTMLUnsafe",
];

crate::ast_check! { on ["assignment_expression", "call_expression", "jsx_attribute", "property_identifier"] => |node, source, ctx, diagnostics|
match node.kind() {
        // el.innerHTML = ... / el.outerHTML = ...
        "assignment_expression" => {
            let Some(lhs) = node.child_by_field_name("left") else { return };
            let Ok(lhs_text) = lhs.utf8_text(source) else { return };
            for prop in ASSIGNMENT_PROPS {
                if lhs_text.ends_with(prop) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-dynamic-template".into(),
                        message: format!(
                            "Dynamic HTML construction via `{}` — use safe DOM APIs or framework escaping instead.",
                            prop,
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
            // location.href = ...
            if lhs_text.ends_with("location.href") || lhs_text == "location.href" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-dynamic-template".into(),
                    message: "Dynamic HTML construction via `location.href =` — use safe DOM APIs or framework escaping instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        // document.write(...), el.insertAdjacentHTML(...), etc.
        "call_expression" => {
            let Some(name) = crate::rules::call_expression::call_function_name(node, source) else { return };
            for method in CALL_METHODS {
                if name == *method || name.ends_with(&format!(".{method}")) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-dynamic-template".into(),
                        message: format!(
                            "Dynamic HTML construction via `{}` — use safe DOM APIs or framework escaping instead.",
                            method,
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
        }
        // JSX: dangerouslySetInnerHTML / v-html
        "jsx_attribute" | "property_identifier" => {
            let Ok(text) = node.utf8_text(source) else { return };
            if text.contains("dangerouslySetInnerHTML") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-dynamic-template".into(),
                    message: "Dynamic HTML construction via `dangerouslySetInnerHTML` — use safe DOM APIs or framework escaping instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_innerhtml() {
        assert_eq!(run_on("el.innerHTML = '<b>' + name + '</b>';").len(), 1);
    }

    #[test]
    fn flags_document_write() {
        assert_eq!(
            run_on("document.write('<script>alert(1)</script>');").len(),
            1
        );
    }

    #[test]
    fn flags_insert_adjacent_html() {
        assert_eq!(run_on("el.insertAdjacentHTML('beforeend', html);").len(), 1);
    }

    #[test]
    fn allows_text_content() {
        assert!(run_on("el.textContent = name;").is_empty());
    }

    #[test]
    fn flags_location_href() {
        assert_eq!(run_on("location.href = userInput;").len(), 1);
    }
}
