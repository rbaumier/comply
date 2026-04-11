//! react-no-danger-with-children text backend.
//!
//! Detects co-occurrence of `dangerouslySetInnerHTML` and `children`
//! (either as a prop or as text content) on the same JSX element.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Scan for JSX elements that contain dangerouslySetInnerHTML.
        // Then check if the same element also has children content or a
        // `children` prop.
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();

            // Find a JSX opening tag
            if let Some(tag_start) = trimmed.find('<') {
                let after = &trimmed[tag_start + 1..];
                // Skip closing tags and fragments
                if !after.starts_with('/') && !after.starts_with('>') {
                    // Collect the element span to check for co-occurrence
                    let mut element_text = String::new();
                    let start_line = i;
                    let mut depth = 0;

                    // Collect lines until the element closes
                    for scan_line in lines.iter().take(lines.len().min(i + 30)).skip(i) {
                        element_text.push_str(scan_line);
                        element_text.push('\n');
                        depth += scan_line.chars().filter(|&c| c == '<').count() as i32;
                        depth -= scan_line.matches("/>").count() as i32;
                        depth -= scan_line.matches("</").count() as i32;

                        let has_danger = element_text.contains("dangerouslySetInnerHTML");
                        let has_children = element_text.contains("children=")
                            || element_text.contains("children:")
                            || has_text_children(&element_text);

                        if has_danger && has_children {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: start_line + 1,
                                column: 1,
                                rule_id: "react-no-danger-with-children".into(),
                                message: "Using both `dangerouslySetInnerHTML` and \
                                          `children` on the same element is invalid — \
                                          React will throw at runtime."
                                    .into(),
                                severity: Severity::Error,
                            });
                            break;
                        }

                        // Self-closing or element closed
                        if trimmed.ends_with("/>") || depth <= 0 {
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
        diagnostics
    }
}

/// Detects text children between JSX tags:
/// `<div dangerouslySetInnerHTML={...}>text here</div>`
fn has_text_children(element: &str) -> bool {
    // Look for pattern: `>text</` — content between closing `>` of open tag
    // and `</` of close tag
    if let Some(close_angle) = element.find('>') {
        let after_open = &element[close_angle + 1..];
        // Skip if it starts with another tag or is empty
        let trimmed = after_open.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with('<')
            && !trimmed.starts_with('{')
            && trimmed.contains("</")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_danger_with_children_prop() {
        let src = r#"<div dangerouslySetInnerHTML={{ __html: html }} children="text" />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_danger_with_text_children() {
        let src = r#"<div dangerouslySetInnerHTML={{ __html: html }}>Some text</div>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_danger_without_children() {
        let src = r#"<div dangerouslySetInnerHTML={{ __html: html }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_children_without_danger() {
        let src = "<div>Some text</div>";
        assert!(run(src).is_empty());
    }
}
