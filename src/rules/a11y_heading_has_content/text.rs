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
            for level in 1..=6 {
                let tag = format!("h{level}");
                let self_close = format!("<{tag} ");
                let self_close2 = format!("<{tag}/>");
                let open = format!("<{tag}>");
                let close = format!("</{tag}>");

                // Self-closing: <h1 ... /> or <h1/>
                if line.contains(&self_close) && line.contains("/>") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-heading-has-content".into(),
                        message: format!("`<{tag}>` is self-closing and has no content."),
                        severity: Severity::Error,
                    });
                } else if line.contains(&self_close2) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-heading-has-content".into(),
                        message: format!("`<{tag}>` is self-closing and has no content."),
                        severity: Severity::Error,
                    });
                }
                // Empty: <h1></h1> on same line
                else if line.contains(&open) && line.contains(&close) {
                    if let Some(start) = line.find(&open) {
                        let content_start = start + open.len();
                        if let Some(end) = line[content_start..].find(&close) {
                            let content = &line[content_start..content_start + end];
                            if content.trim().is_empty() {
                                diagnostics.push(Diagnostic {
                                    path: ctx.path.to_path_buf(),
                                    line: idx + 1,
                                    column: 1,
                                    rule_id: "a11y-heading-has-content".into(),
                                    message: format!("`<{tag}>` is empty and has no content."),
                                    severity: Severity::Error,
                                });
                            }
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
    fn flags_empty_h1() {
        assert_eq!(run("<h1></h1>").len(), 1);
    }

    #[test]
    fn flags_self_closing_h2() {
        assert_eq!(run("<h2 />").len(), 1);
    }

    #[test]
    fn flags_self_closing_h3_compact() {
        assert_eq!(run("<h3/>").len(), 1);
    }

    #[test]
    fn allows_heading_with_content() {
        assert!(run("<h1>Welcome</h1>").is_empty());
    }

    #[test]
    fn flags_empty_h6() {
        assert_eq!(run("<h6></h6>").len(), 1);
    }
}
