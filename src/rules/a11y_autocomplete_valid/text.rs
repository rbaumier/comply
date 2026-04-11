use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

/// Extract the value from `autoComplete="value"` or `autocomplete="value"`.
fn extract_autocomplete_value(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let key = "autocomplete=\"";
    if let Some(pos) = lower.find(key) {
        let start = pos + key.len();
        let rest = &line[start..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(value) = extract_autocomplete_value(line) {
                // Split on whitespace to handle section tokens like "shipping street-address"
                let tokens: Vec<&str> = value.split_whitespace().collect();
                let all_valid = tokens.iter().all(|token| {
                    VALID_AUTOCOMPLETE.contains(&token.to_lowercase().as_str())
                        || token.starts_with("section-")
                });
                if !all_valid {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-autocomplete-valid".into(),
                        message: format!("`autoComplete=\"{}\"` is not a valid autocomplete value.", value),
                        severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_invalid_autocomplete() {
        assert_eq!(run(r#"<input autoComplete="nope" />"#).len(), 1);
    }

    #[test]
    fn allows_valid_autocomplete() {
        assert!(run(r#"<input autoComplete="email" />"#).is_empty());
    }

    #[test]
    fn allows_off() {
        assert!(run(r#"<input autoComplete="off" />"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<input autoComplete="nope" />"#));
        assert!(diags.is_empty());
    }
}
