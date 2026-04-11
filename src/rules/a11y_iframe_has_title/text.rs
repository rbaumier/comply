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
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("<iframe") && !line.contains("title=") {
                // Check next line too for multi-line tags.
                if idx + 1 >= lines.len() || !lines[idx + 1].contains("title=") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-iframe-has-title".into(),
                        message: "`<iframe>` is missing a `title` attribute.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    #[test]
    fn flags_iframe_without_title() {
        assert_eq!(run("<iframe src=\"https://example.com\" />").len(), 1);
    }

    #[test]
    fn allows_iframe_with_title() {
        assert!(run("<iframe src=\"https://example.com\" title=\"Example\" />").is_empty());
    }

    #[test]
    fn allows_iframe_with_title_on_next_line() {
        assert!(run("<iframe src=\"https://example.com\"\n  title=\"Example\" />").is_empty());
    }
}
