//! no-useless-iterator-to-array — flag `.toArray()` in contexts that accept iterables.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

/// `for (... of expr.toArray())`
static FOR_OF_TO_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"for\s*\(.*\bof\b\s+\S+\.toArray\(\)\s*\)").unwrap());

/// `[...expr.toArray()]` or `fn(...expr.toArray())`
static SPREAD_TO_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.\.\.\s*\w[\w.]*\.toArray\(\)").unwrap());

/// `new Set(expr.toArray())`, `new Map(expr.toArray())`, etc.
static NEW_COLLECTION_TO_ARRAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"new\s+(Set|Map|WeakSet|WeakMap)\(\s*\w[\w.]*\.toArray\(\)\s*\)").unwrap()
});

/// `Array.from(expr.toArray())`
static ARRAY_FROM_TO_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Array\.from\(\s*\w[\w.]*\.toArray\(\)\s*\)").unwrap());

/// `Object.fromEntries(expr.toArray())`
static FROM_ENTRIES_TO_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Object\.fromEntries\(\s*\w[\w.]*\.toArray\(\)\s*\)").unwrap());

/// `yield* expr.toArray()`
static YIELD_STAR_TO_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"yield\s*\*\s*\w[\w.]*\.toArray\(\)").unwrap());

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !line.contains(".toArray()") {
                continue;
            }

            let msg = if FOR_OF_TO_ARRAY.is_match(line) {
                "`for...of` can iterate over an iterable, `.toArray()` is unnecessary."
            } else if SPREAD_TO_ARRAY.is_match(line) {
                "Spread works on iterables, `.toArray()` is unnecessary."
            } else if NEW_COLLECTION_TO_ARRAY.is_match(line) {
                "Collection constructor accepts an iterable, `.toArray()` is unnecessary."
            } else if ARRAY_FROM_TO_ARRAY.is_match(line) {
                "`Array.from()` accepts an iterable, `.toArray()` is unnecessary."
            } else if FROM_ENTRIES_TO_ARRAY.is_match(line) {
                "`Object.fromEntries()` accepts an iterable, `.toArray()` is unnecessary."
            } else if YIELD_STAR_TO_ARRAY.is_match(line) {
                "`yield*` can delegate to an iterable, `.toArray()` is unnecessary."
            } else {
                continue;
            };

            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-useless-iterator-to-array".into(),
                message: msg.into(),
                severity: Severity::Warning,
            });
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
    fn flags_for_of_to_array() {
        let d = run("for (const x of iter.toArray()) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("for...of"));
    }

    #[test]
    fn flags_spread_to_array() {
        let d = run("const arr = [...iter.toArray()];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Spread"));
    }

    #[test]
    fn flags_new_set_to_array() {
        let d = run("const s = new Set(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Collection"));
    }

    #[test]
    fn flags_array_from_to_array() {
        let d = run("const a = Array.from(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn allows_standalone_to_array() {
        assert!(run("const arr = iter.toArray();").is_empty());
    }

    #[test]
    fn allows_non_to_array_method() {
        assert!(run("for (const x of iter.values()) {}").is_empty());
    }
}
