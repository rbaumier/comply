use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `[].concat(...arr)` — empty-array concat spread pattern.
fn has_empty_concat_spread(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("[].concat(") {
        let after = start + pos + 10; // after "[].concat("
        let rest = line[after..].trim_start();
        if rest.starts_with("...") {
            return true;
        }
        start = after;
    }
    false
}

/// Detect `.reduce((a, b) => a.concat(b), [])` — reduce-concat pattern.
/// Also catches `(a, b) => [...a, ...b]` variant.
fn has_reduce_concat(line: &str) -> bool {
    // Look for `.reduce(` followed by a concat or spread body and `, [])`
    let mut start = 0;
    while let Some(pos) = line[start..].find(".reduce(") {
        let after = start + pos + 8;
        let rest = &line[after..];
        // Check if the rest of the line contains `.concat(` and ends with `, [])`
        if (rest.contains(".concat(") || rest.contains("[..."))
            && (rest.contains(", [])") || rest.contains(",[])")
                || rest.contains(", [] )"))
        {
            return true;
        }
        start = after;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_concat_spread(line) || has_reduce_concat(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-array-flat".into(),
                    message: "Prefer `.flat()` over legacy array flattening patterns.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_concat_spread() {
        assert_eq!(run("const flat = [].concat(...arr);").len(), 1);
    }

    #[test]
    fn flags_reduce_concat() {
        assert_eq!(
            run("const flat = arr.reduce((a, b) => a.concat(b), []);").len(),
            1
        );
    }

    #[test]
    fn flags_reduce_spread() {
        assert_eq!(
            run("const flat = arr.reduce((a, b) => [...a, ...b], []);").len(),
            1
        );
    }

    #[test]
    fn allows_flat() {
        assert!(run("const flat = arr.flat();").is_empty());
    }

    #[test]
    fn allows_concat_without_spread() {
        assert!(run("const merged = [].concat(arr);").is_empty());
    }

    #[test]
    fn allows_reduce_without_empty_init() {
        assert!(run("const sum = arr.reduce((a, b) => a + b, 0);").is_empty());
    }
}
