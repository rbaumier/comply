//! testing-require-testid-kebab-case backend — detect JSX attributes named
//! `data-testid` / `data-test` whose string value is not kebab-case.
//!
//! Why: mixed casing (camelCase, snake_case, PascalCase) makes queries in
//! tests ambiguous and fragile. Kebab-case is the HTML-attribute-native
//! convention and plays well with CSS-style selectors.

use crate::diagnostic::{Diagnostic, Severity};

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

/// A value is kebab-case if it contains only [a-z0-9-], has no leading/
/// trailing/double hyphen, and is non-empty.
fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() { return false; }
    if s.starts_with('-') || s.ends_with('-') { return false; }
    if s.contains("--") { return false; }
    s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

crate::ast_check! { on ["jsx_attribute"] prefilter = ["data-testid"] => |node, source, ctx, diagnostics|
    // Attribute name is the first child.
    let Some(name_node) = node.named_child(0) else { return; };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name != "data-testid" && name != "data-test" { return; }

    // Value is the second named child.
    let Some(value_node) = node.named_child(1) else { return; };

    // Unwrap `"foo"` (string literal) directly, or `{"foo"}` / `{'foo'}`.
    let literal = match value_node.kind() {
        "string" => value_node,
        "jsx_expression" => {
            let Some(inner) = value_node.named_child(0) else { return; };
            if !matches!(inner.kind(), "string" | "template_string") { return; }
            // Skip templates with interpolation — we can't statically check them.
            if inner.kind() == "template_string" {
                let mut c = inner.walk();
                if inner.named_children(&mut c).any(|n| n.kind() == "template_substitution") {
                    return;
                }
            }
            inner
        }
        _ => return,
    };

    let raw = literal.utf8_text(source).unwrap_or("");
    let value = unquote(raw);

    if !is_kebab_case(value) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &literal,
            super::META.id,
            format!("'{name}=\"{value}\"' is not kebab-case — use lowercase letters, digits, and hyphens only."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_camel_case_testid() {
        assert_eq!(
            run("const x = <button data-testid=\"submitButton\" />;").len(),
            1
        );
    }

    #[test]
    fn flags_snake_case_testid() {
        assert_eq!(
            run("const x = <div data-testid=\"user_card\" />;").len(),
            1
        );
    }

    #[test]
    fn flags_pascal_case_data_test() {
        assert_eq!(
            run("const x = <div data-test=\"UserCard\" />;").len(),
            1
        );
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("const x = <button data-testid=\"submit-button\" />;").is_empty());
    }

    #[test]
    fn allows_kebab_with_digits() {
        assert!(run("const x = <div data-testid=\"row-42\" />;").is_empty());
    }

    #[test]
    fn ignores_dynamic_expression() {
        // {id} can't be checked statically.
        assert!(run("const x = <div data-testid={id} />;").is_empty());
    }
}
