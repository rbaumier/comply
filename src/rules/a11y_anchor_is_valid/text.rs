use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx(ctx: &CheckCtx) -> bool {
    let path = ctx.path.to_string_lossy();
    if path.ends_with(".tsx") || path.ends_with(".jsx") {
        return true;
    }
    let src = ctx.source;
    if src.contains("React") {
        return true;
    }
    src.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'<' && w[1].is_ascii_uppercase())
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            // Only inspect lines with an anchor tag.
            let has_anchor = line.contains("<a ") || line.contains("<a>") || line.contains("<a\t");
            if !has_anchor {
                continue;
            }

            if line.contains("href=\"#\"") || line.contains("href='#'") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-anchor-is-valid".into(),
                    message: "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                    severity: Severity::Error,
                });
            } else if line.contains("href=\"javascript:") || line.contains("href='javascript:") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-anchor-is-valid".into(),
                    message: "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL.".into(),
                    severity: Severity::Error,
                });
            } else if !line.contains("href=") && !line.contains("href=") {
                // No href at all on this line — flag it.
                // (only when we see the opening tag, not just any line)
                if line.contains("/>") || line.contains(">") {
                    // Check the tag is self-contained on this line (has its closing >).
                    if !line.contains("href") {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "a11y-anchor-is-valid".into(),
                            message: "Anchor is missing an `href` attribute.".into(),
                            severity: Severity::Error,
                        });
                    }
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
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    #[test]
    fn flags_href_hash() {
        assert_eq!(run("<a href=\"#\">Click</a>").len(), 1);
    }

    #[test]
    fn flags_href_javascript() {
        assert_eq!(run("<a href=\"javascript:void(0)\">Click</a>").len(), 1);
    }

    #[test]
    fn flags_missing_href() {
        assert_eq!(run("<a onClick={handler}>Click</a>").len(), 1);
    }

    #[test]
    fn allows_valid_href() {
        assert!(run("<a href=\"/home\">Home</a>").is_empty());
    }
}
