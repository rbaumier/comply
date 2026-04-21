//! tailwind-no-important-modifier backend — flag the `!` important modifier
//! inside `className` / `class=` attribute strings. The modifier masks a real
//! specificity bug; callers should fix the cascade, not paper over it.

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
            if let Some(col) = find_important_class(line) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Avoid the Tailwind `!` important modifier — fix specificity instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

/// Return the column of the first `!utility` occurrence on the line, scanning
/// only from the first `className`/`class=` anchor to avoid false positives
/// elsewhere in the line (e.g. plain prose strings).
fn find_important_class(line: &str) -> Option<usize> {
    let start = line.find("className").or_else(|| line.find("class="))?;
    let after = &line[start..];
    let bytes = after.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'!' && bytes[i + 1].is_ascii_lowercase() {
            return Some(start + i);
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
    fn flags_important_class() {
        assert_eq!(run(r#"<div className="!text-red-500 flex" />"#).len(), 1);
    }

    #[test]
    fn flags_important_in_middle() {
        assert_eq!(run(r#"<div className="w-full !hidden" />"#).len(), 1);
    }

    #[test]
    fn allows_normal_classes() {
        assert!(run(r#"<div className="text-red-500 flex" />"#).is_empty());
    }

    #[test]
    fn allows_exclamation_outside_classname() {
        assert!(run(r#"<input placeholder="!important note" />"#).is_empty());
    }
}
