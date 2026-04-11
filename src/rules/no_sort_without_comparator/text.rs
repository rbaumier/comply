use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// `.sort()` or `.sort(  )` — no comparator argument.
fn has_empty_sort(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".sort(") {
        let abs = start + pos + 6; // skip past ".sort("
        // Check if only whitespace before the closing paren.
        let rest = &line[abs..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with(')') {
            return true;
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_sort(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-sort-without-comparator".into(),
                    message: "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_sort() {
        assert_eq!(run("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_sort_with_function() {
        assert!(run("const sorted = arr.sort(compareFn);").is_empty());
    }
}
