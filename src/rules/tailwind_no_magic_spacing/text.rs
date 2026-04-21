//! tailwind-no-magic-spacing backend — flags arbitrary pixel spacing utilities
//! (`p-[13px]`, `gap-[7px]`, etc.) whose numeric value is not a multiple of 4,
//! which breaks consistency with the 4px design-token scale. Non-`px` values
//! (`rem`, `em`, custom props) are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const SPACING_PREFIXES: &[&str] = &[
    "p-[", "px-[", "py-[", "pt-[", "pb-[", "pl-[", "pr-[", "m-[", "mx-[", "my-[", "mt-[", "mb-[",
    "ml-[", "mr-[", "gap-[", "gap-x-[", "gap-y-[", "space-x-[", "space-y-[",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for prefix in SPACING_PREFIXES {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(prefix) {
                    let start = search_from + rel;
                    // Word-boundary: the prefix must not be embedded in a longer
                    // identifier (e.g. `p-[` inside `gap-[`). Skip if the char
                    // immediately before is an ASCII letter or digit.
                    if start > 0
                        && line
                            .as_bytes()
                            .get(start - 1)
                            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
                    {
                        search_from = start + 1;
                        continue;
                    }
                    let inner_start = start + prefix.len();
                    let after = &line[inner_start..];
                    let Some(end_rel) = after.find(']') else { break };
                    let value = &after[..end_rel];
                    if let Some(n) = parse_px(value)
                        && n % 4 != 0
                    {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: i + 1,
                            column: start + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{}{value}]` uses {n}px which is not a multiple of 4 — stick to the design-token spacing scale.",
                                prefix.trim_end_matches('[')
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    search_from = inner_start + end_rel + 1;
                }
            }
        }
        diags
    }
}

/// Parse a value like `13px` as `Some(13)`. Anything that does not end in
/// `px` with only digits before it returns `None`.
fn parse_px(value: &str) -> Option<u64> {
    let stripped = value.strip_suffix("px")?;
    if stripped.is_empty() {
        return None;
    }
    stripped.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_non_multiple_of_four_padding() {
        assert_eq!(run(r#"<div className="p-[13px]" />"#).len(), 1);
    }

    #[test]
    fn flags_margin_seven() {
        assert_eq!(run(r#"<div className="m-[7px]" />"#).len(), 1);
    }

    #[test]
    fn flags_gap_eleven() {
        assert_eq!(run(r#"<div className="gap-[11px]" />"#).len(), 1);
    }

    #[test]
    fn allows_multiple_of_four() {
        assert!(run(r#"<div className="p-[16px]" />"#).is_empty());
    }

    #[test]
    fn allows_rem_unit() {
        assert!(run(r#"<div className="p-[1.5rem]" />"#).is_empty());
    }

    #[test]
    fn allows_standard_scale() {
        assert!(run(r#"<div className="p-4" />"#).is_empty());
    }

    #[test]
    fn skips_commented_line() {
        assert!(run(r#"// <div className="p-[13px]" />"#).is_empty());
    }
}
