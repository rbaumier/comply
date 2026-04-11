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

/// Check if the line contains `tabIndex={N}` where N > 0.
fn has_positive_tabindex(line: &str) -> bool {
    let needle = "tabIndex={";
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos + needle.len();
        if abs < line.len() {
            let ch = line.as_bytes()[abs];
            // Positive: digit 1-9 as the first char after `{`.
            if (b'1'..=b'9').contains(&ch) {
                return true;
            }
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if has_positive_tabindex(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-tabindex-no-positive".into(),
                    message: "`tabIndex` must not be positive — use `0` or `-1` only.".into(),
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
    fn flags_positive_tabindex() {
        assert_eq!(run("<div tabIndex={5} />").len(), 1);
    }

    #[test]
    fn flags_tabindex_1() {
        assert_eq!(run("<input tabIndex={1} />").len(), 1);
    }

    #[test]
    fn allows_tabindex_zero() {
        assert!(run("<div tabIndex={0} />").is_empty());
    }

    #[test]
    fn allows_tabindex_negative() {
        assert!(run("<div tabIndex={-1} />").is_empty());
    }
}
