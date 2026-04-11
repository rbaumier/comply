//! prefer-set-has — flag `const arr = [...]; arr.includes(x)` patterns.
//!
//! This is a heuristic text-based check. It looks for `const` array literal
//! declarations and then searches for `.includes(` calls on that same name.
//! The original eslint rule does full scope analysis; we approximate by
//! scanning the file for the pattern.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

/// Matches `const NAME = [` — a const array literal declaration.
static CONST_ARRAY_DECL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bconst\s+(\w+)\s*=\s*\[").unwrap());

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Phase 1: collect names of const array literals.
        let mut array_names: Vec<String> = Vec::new();
        for line in ctx.source.lines() {
            if let Some(caps) = CONST_ARRAY_DECL.captures(line)
                && let Some(name) = caps.get(1) {
                    array_names.push(name.as_str().to_owned());
                }
        }

        if array_names.is_empty() {
            return diagnostics;
        }

        // Phase 2: look for `.includes(` calls on those names.
        for (idx, line) in ctx.source.lines().enumerate() {
            for name in &array_names {
                let pattern = format!("{name}.includes(");
                if line.contains(&pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "prefer-set-has".into(),
                        message: format!(
                            "`{name}` is a const array used with `.includes()` — consider using a `Set` with `.has()` for O(1) lookups."
                        ),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_const_array_with_includes() {
        let source = "\
const items = [1, 2, 3];
for (const x of data) {
  if (items.includes(x)) {}
}";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("items"));
        assert!(d[0].message.contains("Set"));
    }

    #[test]
    fn flags_multiple_includes_calls() {
        let source = "\
const allowed = ['a', 'b', 'c'];
allowed.includes(x);
allowed.includes(y);";
        let d = run(source);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_let_array_with_includes() {
        let source = "\
let items = [1, 2, 3];
items.includes(1);";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_no_includes_call() {
        let source = "const items = [1, 2, 3];\nconsole.log(items);";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_set_has() {
        let source = "\
const items = new Set([1, 2, 3]);
items.has(1);";
        assert!(run(source).is_empty());
    }
}
