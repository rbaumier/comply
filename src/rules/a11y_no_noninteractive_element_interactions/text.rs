use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const NON_INTERACTIVE: &[&str] = &[
    "<div", "<span", "<p", "<section", "<article", "<header", "<footer", "<main", "<aside", "<nav",
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
            let has_handler = line.contains("onClick=") || line.contains("onKeyDown=");
            if !has_handler {
                continue;
            }
            let has_role = lower.contains("role=");
            if has_role {
                continue;
            }
            for &tag in NON_INTERACTIVE {
                if lower.contains(tag) {
                    // Verify it looks like a tag opening (next char is space, > or /)
                    if let Some(pos) = lower.find(tag) {
                        let after = pos + tag.len();
                        if after >= lower.len()
                            || matches!(
                                lower.as_bytes()[after],
                                b' ' | b'>' | b'/' | b'\t' | b'\n'
                            )
                        {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: pos + 1,
                                rule_id: "a11y-no-noninteractive-element-interactions".into(),
                                message: format!(
                                    "Non-interactive element `{tag}>` has an event handler without a `role` attribute."
                                ),
                                severity: Severity::Warning,
                            });
                            break; // one diagnostic per line
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
    fn flags_div_with_onclick_no_role() {
        let d = run(r#"<div onClick={handler}>Click me</div>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_section_with_onkeydown_no_role() {
        let d = run(r#"<section onKeyDown={handler}>Content</section>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_div_with_onclick_and_role() {
        assert!(run(r#"<div role="button" onClick={handler}>Click</div>"#).is_empty());
    }

    #[test]
    fn allows_button_with_onclick() {
        assert!(run(r#"<button onClick={handler}>Click</button>"#).is_empty());
    }
}
