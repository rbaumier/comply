use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.reverse()` calls — the mutating array method.
fn has_reverse_call(line: &str) -> bool {
    line.contains(".reverse()")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_reverse_call(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-array-reverse".into(),
                    message: "`Array#reverse()` mutates in place — use `.toReversed()` to avoid mutation.".into(),
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
    fn flags_reverse() {
        assert_eq!(run("const rev = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_chained_reverse() {
        assert_eq!(run("arr.filter(x => x > 0).reverse();").len(), 1);
    }

    #[test]
    fn allows_to_reversed() {
        assert!(run("const rev = arr.toReversed();").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run("const x = arr.map(x => x * 2);").is_empty());
    }
}
