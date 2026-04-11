use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DISTRACTING: &[&str] = &["<marquee", "<blink"];

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
            for &tag in DISTRACTING {
                if let Some(pos) = lower.find(tag) {
                    // Ensure it's a tag boundary (next char is space, > or /)
                    let after = pos + tag.len();
                    if after >= lower.len()
                        || matches!(lower.as_bytes()[after], b' ' | b'>' | b'/' | b'\t')
                    {
                        let element = &tag[1..]; // strip '<'
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: pos + 1,
                            rule_id: "a11y-no-distracting-elements".into(),
                            message: format!(
                                "Do not use `<{element}>`. It is deprecated and distracting."
                            ),
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.ts"), source))
    }

    #[test]
    fn flags_marquee() {
        let d = run(r#"<marquee>scrolling text</marquee>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("marquee"));
    }

    #[test]
    fn flags_blink() {
        let d = run(r#"<blink>blinking text</blink>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("blink"));
    }

    #[test]
    fn allows_normal_elements() {
        assert!(run(r#"<div>hello</div>"#).is_empty());
    }

    #[test]
    fn ignores_non_jsx_files() {
        assert!(run_ts(r#"<marquee>text</marquee>"#).is_empty());
    }
}
