use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.match(/.../)` — a regex literal passed to `.match()`.
fn has_match_regex(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".match(") {
        let abs = start + pos + 7; // skip past ".match("
        let rest = &line[abs..].trim_start();
        if rest.starts_with('/') {
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
            if has_match_regex(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-regexp-exec".into(),
                    message: "`.match(/regex/)` is slower — use `regex.exec(string)` instead."
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
    fn flags_match_with_regex() {
        assert_eq!(run("const m = str.match(/foo/);").len(), 1);
    }

    #[test]
    fn flags_match_with_complex_regex() {
        assert_eq!(run("const m = input.match(/^[a-z]+$/i);").len(), 1);
    }

    #[test]
    fn allows_match_with_variable() {
        assert!(run("const m = str.match(pattern);").is_empty());
    }

    #[test]
    fn allows_exec() {
        assert!(run("const m = /foo/.exec(str);").is_empty());
    }
}
