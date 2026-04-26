//! a11y-autocomplete-valid — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

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

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if let Some(value) = attr_value(elem.attrs, "autocomplete") {
                let all_valid = value.split_whitespace().all(|token| {
                    VALID_AUTOCOMPLETE.contains(&token.to_lowercase().as_str())
                        || token.starts_with("section-")
                });
                if !all_valid {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-autocomplete-valid".into(),
                        message: format!("`autocomplete=\"{value}\"` is not a valid autocomplete value."),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <input autocomplete=\"nope\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_valid_autocomplete() {
        let source = "<template>\n  <input autocomplete=\"email\" />\n</template>";
        assert!(run(source).is_empty());
    }
}
