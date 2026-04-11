//! ts-no-unnecessary-type-constraint backend — scan for `extends any` or
//! `extends unknown` in generic type parameter positions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Match `extends any` or `extends unknown` in generic context.
    // Require a word boundary after `any`/`unknown` to avoid partial matches.
    Regex::new(r"\bextends\s+(any|unknown)\b").unwrap()
});

/// Check if the match is inside a generic type parameter context (after `<`).
fn is_in_generic_context(line: &str, match_start: usize) -> bool {
    let before = &line[..match_start];
    // Count angle brackets — if there's an unmatched `<`, we're in a generic context.
    let mut depth = 0i32;
    for ch in before.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for m in PATTERN.find_iter(line) {
                if !is_in_generic_context(line, m.start()) {
                    continue;
                }
                let constraint = if m.as_str().contains("unknown") {
                    "unknown"
                } else {
                    "any"
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: m.start() + 1,
                    rule_id: "ts-no-unnecessary-type-constraint".into(),
                    message: format!(
                        "Unnecessary `extends {constraint}` constraint — \
                         all types already extend `{constraint}`."
                    ),
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
    fn flags_extends_any() {
        let diags = run("function f<T extends any>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`any`"));
    }

    #[test]
    fn flags_extends_unknown() {
        let diags = run("function f<T extends unknown>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`unknown`"));
    }

    #[test]
    fn allows_extends_string() {
        assert!(run("function f<T extends string>(x: T): T { return x; }").is_empty());
    }

    #[test]
    fn ignores_extends_in_class() {
        // `extends` in class inheritance is not a generic constraint
        assert!(run("class Foo extends any {}").is_empty());
    }
}
