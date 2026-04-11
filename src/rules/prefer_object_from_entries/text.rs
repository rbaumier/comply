use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.reduce(` followed by `{}` or `Object.create(null)` as the
/// initial value, which is a strong signal of building an object from pairs.
///
/// This is a heuristic: we look for `.reduce(` with a second argument that
/// is `{}` or `Object.create(null)` on the same line.
fn find_reduce_to_object(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".reduce(") {
        let abs = start + pos;
        let after = &line[abs + 8..];
        // Look for `{})` or `{} )` or `Object.create(null)` somewhere after `.reduce(`
        if has_empty_object_init(after) {
            hits.push(abs);
        }
        start = abs + 8;
    }
    hits
}

/// Check whether the remainder of the line (after `.reduce(`) contains
/// an empty-object initializer: `, {})` or `, Object.create(null))`.
fn has_empty_object_init(s: &str) -> bool {
    // Pattern 1: `, {})`  — possibly with whitespace
    if s.contains(", {})") || s.contains(",{})") {
        return true;
    }
    // Pattern 2: `, Object.create(null))`
    if s.contains("Object.create(null)") {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_reduce_to_object(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-object-from-entries".into(),
                    message:
                        "Prefer `Object.fromEntries()` over `Array#reduce()` to build an object."
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
    fn flags_reduce_with_empty_object() {
        let d = run("const obj = pairs.reduce((acc, [k, v]) => ({ ...acc, [k]: v }), {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reduce_with_object_create_null() {
        let d = run(
            "const obj = pairs.reduce((acc, [k, v]) => { acc[k] = v; return acc; }, Object.create(null));",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_reduce_with_non_object_init() {
        assert!(run("const sum = nums.reduce((acc, n) => acc + n, 0);").is_empty());
    }

    #[test]
    fn allows_object_from_entries() {
        assert!(run("const obj = Object.fromEntries(pairs.map(([k, v]) => [k, v]));").is_empty());
    }

    #[test]
    fn allows_reduce_with_array_init() {
        assert!(run("const arr = items.reduce((acc, x) => [...acc, x], []);").is_empty());
    }
}
