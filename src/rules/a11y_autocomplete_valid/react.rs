//! a11y-autocomplete-valid backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

const VALID_AUTOCOMPLETE: &[&str] = &[
    "on", "off", "name", "honorific-prefix", "given-name", "additional-name",
    "family-name", "honorific-suffix", "nickname", "email", "username",
    "new-password", "current-password", "one-time-code", "organization-title",
    "organization", "street-address", "address-line1", "address-line2",
    "address-line3", "address-level4", "address-level3", "address-level2",
    "address-level1", "country", "country-name", "postal-code", "cc-name",
    "cc-given-name", "cc-additional-name", "cc-family-name", "cc-number",
    "cc-exp", "cc-exp-month", "cc-exp-year", "cc-csc", "cc-type",
    "transaction-currency", "transaction-amount", "language", "bday",
    "bday-day", "bday-month", "bday-year", "sex", "tel", "tel-country-code",
    "tel-national", "tel-area-code", "tel-local", "tel-extension", "impp",
    "url", "photo",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else {
        return;
    };
    // JSX uses camelCase autoComplete, but also match lowercase
    if name != "autoComplete" && name != "autocomplete" { return; }

    let Some(val_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    let Ok(val_text) = val_node.utf8_text(source) else { return };

    // Strip surrounding quotes
    let value = val_text.trim_matches('"').trim_matches('\'');

    // Split on whitespace to handle section tokens like "shipping street-address"
    let all_valid = value.split_whitespace().all(|token| {
        VALID_AUTOCOMPLETE.contains(&token.to_lowercase().as_str())
            || token.starts_with("section-")
    });

    if !all_valid {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-autocomplete-valid".into(),
            message: format!("`autoComplete=\"{value}\"` is not a valid autocomplete value."),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_invalid_autocomplete() {
        assert_eq!(run_on(r#"const x = <input autoComplete="nope" />;"#).len(), 1);
    }

    #[test]
    fn allows_valid_autocomplete() {
        assert!(run_on(r#"const x = <input autoComplete="email" />;"#).is_empty());
    }

    #[test]
    fn allows_off() {
        assert!(run_on(r#"const x = <input autoComplete="off" />;"#).is_empty());
    }
}
