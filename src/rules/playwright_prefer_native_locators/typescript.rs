//! playwright-prefer-native-locators AST backend — flag `locator('[role="..."]')` etc.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Attribute selectors inside `.locator()` that have native equivalents.
const ATTRIBUTE_SELECTORS: &[(&str, &str)] = &[
    ("[role=", "getByRole"),
    ("[placeholder=", "getByPlaceholder"),
    ("[alt=", "getByAltText"),
    ("[title=", "getByTitle"),
    ("[data-testid=", "getByTestId"),
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "locator" {
        return;
    }

    // Extract the first string argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor)
        .find(|c| c.kind() == "string" || c.kind() == "template_string");

    let Some(arg) = first_arg else { return };
    let text = arg.utf8_text(source).unwrap_or("");

    for &(attr, replacement) in ATTRIBUTE_SELECTORS {
        if text.contains(attr) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "playwright-prefer-native-locators".into(),
                message: format!(
                    "Attribute selector `{attr}...]` in `.locator()` — \
                     use `{replacement}()` instead."
                ),
                severity: Severity::Warning,
            });
            break; // One diagnostic per locator call.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, "login.test.ts")
    }

    #[test]
    fn flags_role_attribute_selector() {
        let d = run_on(r#"page.locator('[role="button"]');"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-native-locators");
        assert!(d[0].message.contains("getByRole"));
    }

    #[test]
    fn flags_data_testid_attribute() {
        let d = run_on(r#"page.locator('[data-testid="card"]');"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getByTestId"));
    }

    #[test]
    fn allows_get_by_role() {
        assert!(run_on("page.getByRole('button');").is_empty());
    }

    #[test]
    fn allows_locator_without_attribute() {
        assert!(run_on("page.locator('button');").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_ts_with_path(
            r#"page.locator('[role="button"]');"#,
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
