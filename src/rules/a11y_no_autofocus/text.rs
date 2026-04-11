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
            // Match JSX camelCase `autoFocus` (not HTML lowercase `autofocus`).
            if line.contains("autoFocus") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-no-autofocus".into(),
                    message: "Avoid `autoFocus` — it is disorienting for screen reader users.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_autofocus() {
        assert_eq!(run("<input autoFocus />").len(), 1);
    }

    #[test]
    fn flags_autofocus_with_value() {
        assert_eq!(run("<input autoFocus={true} />").len(), 1);
    }

    #[test]
    fn allows_input_without_autofocus() {
        assert!(run("<input type=\"text\" />").is_empty());
    }
}
