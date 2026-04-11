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
            if line.contains("<html") && !line.contains("lang=") {
                // Also check the next line for a multi-line tag.
                if idx + 1 >= lines.len() || !lines[idx + 1].contains("lang=") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-html-has-lang".into(),
                        message: "`<html>` is missing a `lang` attribute.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("layout.tsx"), source))
    }

    #[test]
    fn flags_html_without_lang() {
        assert_eq!(run("<html>").len(), 1);
    }

    #[test]
    fn allows_html_with_lang() {
        assert!(run("<html lang=\"en\">").is_empty());
    }

    #[test]
    fn allows_html_with_lang_on_next_line() {
        assert!(run("<html\n  lang=\"en\">").is_empty());
    }
}
