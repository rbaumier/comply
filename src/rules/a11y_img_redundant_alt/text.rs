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

/// Extract the value of an `alt="..."` attribute from a line.
fn extract_alt_value(line: &str) -> Option<&str> {
    // Try double quotes.
    if let Some(pos) = line.find("alt=\"") {
        let start = pos + 5;
        if let Some(end) = line[start..].find('"') {
            return Some(&line[start..start + end]);
        }
    }
    // Try single quotes.
    if let Some(pos) = line.find("alt='") {
        let start = pos + 5;
        if let Some(end) = line[start..].find('\'') {
            return Some(&line[start..start + end]);
        }
    }
    None
}

/// Check if the alt text contains redundant words.
fn has_redundant_word(alt: &str) -> bool {
    let lower = alt.to_ascii_lowercase();
    // Check for whole-word occurrences (or substring — common lint behavior).
    lower.contains("image") || lower.contains("picture") || lower.contains("photo")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("<img")
                && let Some(alt) = extract_alt_value(line)
                && has_redundant_word(alt)
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-img-redundant-alt".into(),
                    message: "`alt` text should not contain words like \"image\", \"picture\", or \"photo\" — describe the content instead.".into(),
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
    fn flags_alt_with_image() {
        assert_eq!(run("<img alt=\"An image of a cat\" src=\"cat.png\" />").len(), 1);
    }

    #[test]
    fn flags_alt_with_photo_case_insensitive() {
        assert_eq!(run("<img alt=\"Photo of sunset\" src=\"sunset.png\" />").len(), 1);
    }

    #[test]
    fn flags_alt_with_picture() {
        assert_eq!(run("<img alt=\"A picture\" src=\"pic.png\" />").len(), 1);
    }

    #[test]
    fn allows_descriptive_alt() {
        assert!(run("<img alt=\"A golden retriever playing fetch\" src=\"dog.png\" />").is_empty());
    }
}
