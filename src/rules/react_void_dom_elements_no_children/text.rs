//! react-void-dom-elements-no-children text backend.
//!
//! Flags void HTML elements that have children (text or props).
//! Void elements: area, base, br, col, embed, hr, img, input, keygen,
//! link, meta, param, source, track, wbr.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen",
    "link", "meta", "param", "source", "track", "wbr",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            for &el in VOID_ELEMENTS {
                // Check for `<br>content</br>` or `<img children={...}>`
                // or `<br dangerouslySetInnerHTML=...>`
                let open_tag = format!("<{el}");
                if !trimmed.contains(&open_tag) {
                    continue;
                }

                // Pattern 1: closing tag `</br>` on same or nearby line —
                // means non-self-closing void element
                let close_tag = format!("</{el}>");
                if trimmed.contains(&close_tag) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-void-dom-elements-no-children".into(),
                        message: format!(
                            "`<{el}>` is a void element and cannot have children."
                        ),
                        severity: Severity::Error,
                    });
                    continue;
                }

                // Pattern 2: `children=` or `dangerouslySetInnerHTML=` prop
                if let Some(pos) = trimmed.find(&open_tag) {
                    let rest = &trimmed[pos..];
                    if rest.contains("children=")
                        || rest.contains("dangerouslySetInnerHTML=")
                    {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "react-void-dom-elements-no-children".into(),
                            message: format!(
                                "`<{el}>` is a void element and cannot have children."
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_br_with_children() {
        let src = "<br>text</br>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_img_with_children_prop() {
        let src = r#"<img children={<span />} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hr_with_danger() {
        let src = r#"<hr dangerouslySetInnerHTML={{ __html: "x" }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_self_closing_void() {
        let src = r#"<br />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_div_with_children() {
        let src = "<div>text</div>";
        assert!(run(src).is_empty());
    }
}
