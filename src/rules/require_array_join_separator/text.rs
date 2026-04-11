use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// `.join()` or `.join(  )` — no separator argument.
fn has_empty_join(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".join(") {
        let abs = start + pos + 6; // skip past ".join("
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
            if has_empty_join(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "require-array-join-separator".into(),
                    message: "Missing the separator argument in `.join()` — use `.join(',')` explicitly.".into(),
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
    fn flags_empty_join() {
        assert_eq!(run("const s = arr.join();").len(), 1);
    }

    #[test]
    fn flags_join_with_whitespace() {
        assert_eq!(run("const s = arr.join(  );").len(), 1);
    }

    #[test]
    fn allows_join_with_separator() {
        assert!(run("const s = arr.join(',');").is_empty());
    }

    #[test]
    fn allows_join_with_variable() {
        assert!(run("const s = arr.join(sep);").is_empty());
    }

    #[test]
    fn flags_chained_join() {
        assert_eq!(run("foo.map(x => x.id).join()").len(), 1);
    }
}
