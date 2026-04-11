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
            // Self-closing: <a ... />
            if (line.contains("<a ") || line.contains("<a\t"))
                && line.contains("/>") && !line.contains("aria-label")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-anchor-has-content".into(),
                    message: "Anchor is self-closing and has no content for screen readers.".into(),
                    severity: Severity::Error,
                });
            }

            // Empty: <a ...></a> on the same line
            if let Some(open_pos) = line.find("<a ").or_else(|| line.find("<a>"))
                && let Some(close_pos) = line.find("</a>")
            {
                // Find the end of the opening tag.
                if let Some(gt) = line[open_pos..].find('>') {
                    let content_start = open_pos + gt + 1;
                    if content_start <= close_pos {
                        let content = line[content_start..close_pos].trim();
                        if content.is_empty() && !line.contains("aria-label") {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: 1,
                                rule_id: "a11y-anchor-has-content".into(),
                                message: "Anchor has no content — screen readers cannot announce it.".into(),
                                severity: Severity::Error,
                            });
                        }
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
    fn flags_self_closing_anchor() {
        assert_eq!(run("<a href=\"/home\" />").len(), 1);
    }

    #[test]
    fn flags_empty_anchor() {
        assert_eq!(run("<a href=\"/home\"></a>").len(), 1);
    }

    #[test]
    fn allows_anchor_with_content() {
        assert!(run("<a href=\"/home\">Home</a>").is_empty());
    }

    #[test]
    fn allows_anchor_with_aria_label() {
        assert!(run("<a href=\"/home\" aria-label=\"Home\" />").is_empty());
    }
}
