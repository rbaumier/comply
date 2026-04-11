//! no-disable-mustache-escape — flag disabling of HTML escaping in
//! template engines (e.g. `escapeMarkup = false`, `noEscape: true`).
//!
//! Matches `assignment_expression` and `pair` (object property) nodes
//! that set escape-related properties to dangerous values.

use crate::diagnostic::{Diagnostic, Severity};

const DISABLE_PATTERNS: &[(&str, &str, &str)] = &[
    // (property_name, bad_value, pattern_description)
    ("escapeMarkup", "false", "escapeMarkup = false"),
    ("escape", "false", "escape = false"),
    ("noEscape", "true", "noEscape: true"),
];

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        // Handle: `x.escapeMarkup = false` or `escapeMarkup = false`
        "assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return };
            let Some(right) = node.child_by_field_name("right") else { return };

            let prop_name = match left.kind() {
                "member_expression" => {
                    left.child_by_field_name("property")
                        .and_then(|p| p.utf8_text(source).ok())
                }
                "identifier" => left.utf8_text(source).ok(),
                _ => None,
            };
            let Some(prop) = prop_name else { return };
            let Ok(val) = right.utf8_text(source) else { return };

            for &(name, bad_val, desc) in DISABLE_PATTERNS {
                if prop == name && val == bad_val {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-disable-mustache-escape".into(),
                        message: format!(
                            "Disabling HTML escaping via `{}` — keep escaping enabled to prevent XSS.",
                            desc,
                        ),
                        severity: Severity::Error,
                    });
                    return;
                }
            }
        }
        // Handle: `{ escapeMarkup: false }` or `{ noEscape: true }`
        "pair" => {
            let Some(key) = node.child_by_field_name("key") else { return };
            let Some(value) = node.child_by_field_name("value") else { return };
            let Ok(key_text) = key.utf8_text(source) else { return };
            let Ok(val_text) = value.utf8_text(source) else { return };

            for &(name, bad_val, desc) in DISABLE_PATTERNS {
                if key_text == name && val_text == bad_val {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-disable-mustache-escape".into(),
                        message: format!(
                            "Disabling HTML escaping via `{}` — keep escaping enabled to prevent XSS.",
                            desc,
                        ),
                        severity: Severity::Error,
                    });
                    return;
                }
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
    fn flags_escape_markup_false_assignment() {
        assert_eq!(run_on("options.escapeMarkup = false;").len(), 1);
    }

    #[test]
    fn flags_escape_markup_property() {
        assert_eq!(run_on("const x = { escapeMarkup: false };").len(), 1);
    }

    #[test]
    fn flags_no_escape_true() {
        assert_eq!(run_on("const x = { noEscape: true };").len(), 1);
    }

    #[test]
    fn allows_escape_enabled() {
        assert!(run_on("const x = { escapeMarkup: true };").is_empty());
    }
}
