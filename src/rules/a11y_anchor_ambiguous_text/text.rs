use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const AMBIGUOUS_TEXTS: &[&str] = &[
    "click here",
    "here",
    "link",
    "a link",
    "read more",
    "learn more",
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
            let lower = line.to_lowercase();
            if !lower.contains("<a") {
                continue;
            }
            for &text in AMBIGUOUS_TEXTS {
                // Match >ambiguous text< pattern (case-insensitive)
                let pattern = format!(">{text}<");
                if lower.contains(&pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-anchor-ambiguous-text".into(),
                        message: format!(
                            "Ambiguous link text \"{text}\". Use descriptive text that indicates the link's purpose."
                        ),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per line
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
    fn flags_click_here() {
        let d = run(r#"<a href="/page">click here</a>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("click here"));
    }

    #[test]
    fn flags_read_more_case_insensitive() {
        let d = run(r#"<a href="/page">Read More</a>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_descriptive_text() {
        assert!(run(r#"<a href="/docs">View documentation</a>"#).is_empty());
    }

    #[test]
    fn flags_here() {
        let d = run(r#"<a href="/page">here</a>"#);
        assert_eq!(d.len(), 1);
    }
}
