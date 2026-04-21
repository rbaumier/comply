//! tailwind-prefer-size-shorthand backend — flag `w-X h-X` pairs with matching
//! values so the caller can collapse them to the Tailwind v3.4+ `size-X`
//! shorthand.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") {
                continue;
            }
            if let Some(val) = find_wh_duplicate(line) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Replace `w-{val} h-{val}` with `size-{val}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

/// Extract the text inside every `className="..."` / `class="..."` attribute
/// on the line. Returns the unquoted inner strings.
fn class_strings(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for attr in ["className=\"", "class=\""] {
        let mut from = 0;
        while let Some(idx) = line[from..].find(attr) {
            let start = from + idx + attr.len();
            if let Some(end) = line[start..].find('"') {
                out.push(&line[start..start + end]);
                from = start + end + 1;
            } else {
                break;
            }
        }
    }
    out
}

/// Scan the class strings for a matching `w-X` / `h-X` pair.
fn find_wh_duplicate(line: &str) -> Option<String> {
    for class_str in class_strings(line) {
        let tokens: Vec<&str> = class_str.split_whitespace().collect();
        let w_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("w-")).collect();
        let h_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("h-")).collect();
        for w in &w_vals {
            if h_vals.contains(w) {
                return Some((*w).to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_equal_w_h() {
        assert_eq!(run(r#"<div className="w-4 h-4 flex" />"#).len(), 1);
    }

    #[test]
    fn flags_full() {
        assert_eq!(run(r#"<div className="w-full h-full" />"#).len(), 1);
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"<div className="w-4 h-6" />"#).is_empty());
    }

    #[test]
    fn allows_size_shorthand_already() {
        assert!(run(r#"<div className="size-4" />"#).is_empty());
    }
}
