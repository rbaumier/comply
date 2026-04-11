//! react-checked-requires-onchange text backend.
//!
//! Flags `<input checked={...}>` without `onChange` or `readOnly`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let lower = line.trim().to_ascii_lowercase();

            // Look for `<input` elements with `checked`
            if lower.contains("<input") && lower.contains("checked") {
                // Gather the full element
                let mut element = String::new();
                for scan_line in lines.iter().take(lines.len().min(i + 10)).skip(i) {
                    element.push_str(scan_line);
                    element.push(' ');
                    if scan_line.contains("/>") || scan_line.trim().ends_with('>') {
                        break;
                    }
                }

                let lower_elem = element.to_ascii_lowercase();
                // Only flag if it has `checked` but not `onChange` or `readOnly`
                if lower_elem.contains("checked")
                    && !lower_elem.contains("defaultchecked")
                    && !lower_elem.contains("onchange")
                    && !lower_elem.contains("readonly")
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: "react-checked-requires-onchange".into(),
                        message: "`checked` without `onChange` or `readOnly` renders \
                                  a frozen input. Add an `onChange` handler or \
                                  `readOnly`."
                            .into(),
                        severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_checked_without_onchange() {
        let src = r#"<input type="checkbox" checked={isChecked} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_checked_with_onchange() {
        let src = r#"<input type="checkbox" checked={isChecked} onChange={handleChange} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_readonly() {
        let src = r#"<input type="checkbox" checked={isChecked} readOnly />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_checked() {
        let src = r#"<input type="checkbox" defaultChecked={true} />"#;
        assert!(run(src).is_empty());
    }
}
