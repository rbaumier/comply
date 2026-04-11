use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects any `.sort(` call — flags ALL `.sort()` usage (with or without
/// comparator) because the mutation is the problem, not the missing comparator.
fn has_sort_call(line: &str) -> bool {
    line.contains(".sort(")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_sort_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-array-sort-mutation".into(),
                    message: "Use `.toSorted()` instead of `.sort()` — \
                              `sort()` mutates the array in place."
                        .into(),
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
    fn flags_sort_without_comparator() {
        assert_eq!(run("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_comparator() {
        assert_eq!(run("arr.sort((a, b) => a - b);").len(), 1);
    }

    #[test]
    fn allows_to_sorted() {
        assert!(run("const sorted = arr.toSorted();").is_empty());
    }

    #[test]
    fn allows_to_sorted_with_comparator() {
        assert!(run("const sorted = arr.toSorted((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// arr.sort() is bad").is_empty());
    }

    #[test]
    fn flags_chained_sort() {
        assert_eq!(run("const sorted = items.filter(x => x).sort();").len(), 1);
    }
}
