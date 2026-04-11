//! ts-no-non-null-asserted-nullish-coalescing backend — scan for `! ??` pattern.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\s*\?\?").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for m in PATTERN.find_iter(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: m.start() + 1,
                    rule_id: "ts-no-non-null-asserted-nullish-coalescing".into(),
                    message: "`x! ?? y` is contradictory — the `!` asserts non-null \
                              while `??` handles null. Remove the `!`."
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
    fn flags_non_null_with_nullish_coalescing() {
        let diags = run("const x = value! ?? 'default';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_no_space() {
        let diags = run("const x = value!??'default';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_nullish_coalescing_without_non_null() {
        assert!(run("const x = value ?? 'default';").is_empty());
    }

    #[test]
    fn allows_non_null_without_nullish_coalescing() {
        assert!(run("const x = value!;").is_empty());
    }
}
