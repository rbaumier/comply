use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const INTERACTIVE_ELEMENTS: &[&str] = &["<button", "<input", "<select", "<textarea"];

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
        let lines: Vec<&str> = ctx.source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();
            let has_interactive = INTERACTIVE_ELEMENTS.iter().any(|el| lower.contains(el));
            if !has_interactive {
                continue;
            }
            // Self-closing <input with type="hidden" is exempt
            if lower.contains("<input") && lower.contains("type=\"hidden\"") {
                continue;
            }
            // Check within a 2-line window for labels
            let window_end = std::cmp::min(idx + 3, lines.len());
            let window = lines[idx..window_end].join(" ");
            let window_lower = window.to_lowercase();
            if window_lower.contains("aria-label=")
                || window_lower.contains("aria-labelledby=")
            {
                continue;
            }
            // For <button>, check if it has text content (non-self-closing with content)
            // Simple heuristic: if it's self-closing (/>), it needs a label
            // If it has content between tags on the same line, it's fine
            if lower.contains("<button") && !lower.contains("/>") {
                // Has potential text content — skip
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "a11y-control-has-associated-label".into(),
                message: "Interactive element is missing an accessible label (`aria-label` or `aria-labelledby`).".into(),
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
    fn flags_button_without_label() {
        assert_eq!(run(r#"<button />"#).len(), 1);
    }

    #[test]
    fn flags_input_without_label() {
        assert_eq!(run(r#"<input />"#).len(), 1);
    }

    #[test]
    fn allows_input_with_aria_label() {
        assert!(run(r#"<input aria-label="Name" />"#).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        assert!(run(r#"<input type="hidden" />"#).is_empty());
    }

    #[test]
    fn allows_button_with_text_content() {
        assert!(run(r#"<button>Submit</button>"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<button />"#));
        assert!(diags.is_empty());
    }
}
