use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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
            if !lower.contains("<label") {
                continue;
            }
            // Check same line for htmlFor= or for=
            if line.contains("htmlFor=") || lower.contains(" for=") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "a11y-label-has-associated-control".into(),
                message: "`<label>` is missing `htmlFor` — associate it with a form control.".into(),
                severity: Severity::Warning,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_label_without_htmlfor() {
        assert_eq!(run(r#"<label>Name</label>"#).len(), 1);
    }

    #[test]
    fn allows_label_with_htmlfor() {
        assert!(run(r#"<label htmlFor="name-input">Name</label>"#).is_empty());
    }

    #[test]
    fn allows_label_with_for() {
        assert!(run(r#"<label for="name-input">Name</label>"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<label>Name</label>"#));
        assert!(diags.is_empty());
    }
}
