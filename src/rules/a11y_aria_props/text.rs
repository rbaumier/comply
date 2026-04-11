use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VALID_ARIA: &[&str] = &[
    "aria-activedescendant",
    "aria-atomic",
    "aria-autocomplete",
    "aria-busy",
    "aria-checked",
    "aria-colcount",
    "aria-colindex",
    "aria-colspan",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-expanded",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-hidden",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-multiline",
    "aria-multiselectable",
    "aria-orientation",
    "aria-owns",
    "aria-placeholder",
    "aria-posinset",
    "aria-pressed",
    "aria-readonly",
    "aria-relevant",
    "aria-required",
    "aria-roledescription",
    "aria-rowcount",
    "aria-rowindex",
    "aria-rowspan",
    "aria-selected",
    "aria-setsize",
    "aria-sort",
    "aria-valuemax",
    "aria-valuemin",
    "aria-valuenow",
    "aria-valuetext",
];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let mut start = 0;
            while let Some(pos) = line[start..].find("aria-") {
                let abs = start + pos;
                // Extract the full attribute name (aria-xxx up to = or whitespace)
                let attr_end = line[abs..]
                    .find(|c: char| c == '=' || c.is_whitespace() || c == '>' || c == '/')
                    .map(|i| abs + i)
                    .unwrap_or(line.len());
                let attr = &line[abs..attr_end];
                if !attr.is_empty() && !VALID_ARIA.contains(&attr) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: abs + 1,
                        rule_id: "a11y-aria-props".into(),
                        message: format!("Invalid ARIA attribute `{attr}`. Use a valid WAI-ARIA attribute."),
                        severity: Severity::Error,
                    });
                }
                start = attr_end;
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
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.ts"), source))
    }

    #[test]
    fn flags_invalid_aria_attribute() {
        let d = run(r#"<div aria-invalid-attr="true">"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("aria-invalid-attr"));
    }

    #[test]
    fn allows_valid_aria_attributes() {
        assert!(run(r#"<div aria-label="hello" aria-hidden="true">"#).is_empty());
    }

    #[test]
    fn ignores_non_jsx_files() {
        assert!(run_ts(r#"<div aria-fakeprop="true">"#).is_empty());
    }
}
