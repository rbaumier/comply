use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` if the source likely contains JSX.
// REVIEW: à extraire dans une fonction réutilisable
fn is_jsx(ctx: &CheckCtx) -> bool {
    let path = ctx.path.to_string_lossy();
    if path.ends_with(".tsx") || path.ends_with(".jsx") {
        return true;
    }
    let src = ctx.source;
    if src.contains("React") {
        return true;
    }
    // `<` followed by an uppercase letter — JSX element.
    src.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'<' && w[1].is_ascii_uppercase())
}

/// Check if the tag on this line (and optionally the next) contains the given attribute.
fn has_attr_in_window(lines: &[&str], idx: usize, attr: &str) -> bool {
    // Check current line.
    if lines[idx].contains(attr) {
        return true;
    }
    // Check next line.
    if idx + 1 < lines.len() && lines[idx + 1].contains(attr) {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            // <img without alt=
            if (line.contains("<img ")
                || line.contains("<img\n")
                || line.trim_end().ends_with("<img"))
                && !has_attr_in_window(&lines, idx, "alt=")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-alt-text".into(),
                    message: "`<img>` is missing an `alt` attribute.".into(),
                    severity: Severity::Error,
                });
            }

            // <area without alt=
            if (line.contains("<area ") || line.trim_end().ends_with("<area"))
                && !has_attr_in_window(&lines, idx, "alt=")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-alt-text".into(),
                    message: "`<area>` is missing an `alt` attribute.".into(),
                    severity: Severity::Error,
                });
            }

            // <input type="image" without alt=
            if line.contains("<input")
                && line.contains("type=\"image\"")
                && !has_attr_in_window(&lines, idx, "alt=")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-alt-text".into(),
                    message: "`<input type=\"image\">` is missing an `alt` attribute.".into(),
                    severity: Severity::Error,
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
    fn flags_img_without_alt() {
        assert_eq!(run("<img src=\"logo.png\" />").len(), 1);
    }

    #[test]
    fn allows_img_with_alt() {
        assert!(run("<img alt=\"Logo\" src=\"logo.png\" />").is_empty());
    }

    #[test]
    fn flags_area_without_alt() {
        assert_eq!(run("<area shape=\"rect\" />").len(), 1);
    }

    #[test]
    fn flags_input_type_image_without_alt() {
        assert_eq!(run("<input type=\"image\" src=\"btn.png\" />").len(), 1);
    }

    #[test]
    fn allows_input_type_image_with_alt() {
        assert!(run("<input type=\"image\" alt=\"Submit\" src=\"btn.png\" />").is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("plain.ts"), "const x = 1;"));
        assert!(diags.is_empty());
    }
}
