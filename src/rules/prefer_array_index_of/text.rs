use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.findIndex(x => x === val)` — simple equality callback that should
/// be `.indexOf(val)`.
///
/// Matches arrow forms:
///   `.findIndex(x => x === something)`
///   `.findIndex(x => something === x)`
///   `.findIndex((x) => x === something)`
///
/// Also detects `.findLastIndex(…)` with the same pattern.
fn has_simple_find_index(line: &str) -> bool {
    for method in &[".findIndex(", ".findLastIndex("] {
        let mut start = 0;
        while let Some(pos) = line[start..].find(method) {
            let after = start + pos + method.len();
            let rest = &line[after..];
            // Extract the callback body until the closing paren.
            // We need to find a simple `param => param === expr` or
            // `(param) => param === expr` pattern.
            if is_simple_equality_callback(rest) {
                return true;
            }
            start = after;
        }
    }
    false
}

/// Check if text starting at the callback argument position is a simple
/// equality arrow: `x => x === val)` or `(x) => x === val)`.
fn is_simple_equality_callback(s: &str) -> bool {
    is_simple_equality_callback_inner(s).unwrap_or(false)
}

fn is_simple_equality_callback_inner(s: &str) -> Option<bool> {
    let s = s.trim_start();

    // Extract param name — either bare `x` or `(x)`
    let (param, rest) = if s.starts_with('(') {
        // `(x) => ...`
        let close = s.find(')')?;
        let param = s[1..close].trim();
        if param.is_empty() || param.contains(',') {
            return Some(false);
        }
        (param, &s[close + 1..])
    } else {
        // `x => ...`
        let arrow = s.find("=>")?;
        let param = s[..arrow].trim();
        if param.is_empty() || param.contains(',') || param.contains('(') {
            return Some(false);
        }
        (param, &s[arrow..])
    };

    // Expect ` => ` next
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("=>")?;
    let rest = rest.trim_start();

    // Body should be `param === something` or `something === param`
    // followed by `)` (end of findIndex call)
    if let Some(eq_pos) = rest.find("===") {
        let left = rest[..eq_pos].trim();
        let after_eq = &rest[eq_pos + 3..];
        // Find the closing paren of findIndex
        let close = find_callback_end(after_eq)?;
        let right = after_eq[..close].trim();
        // One side must be the parameter
        if left == param || right == param {
            return Some(true);
        }
    }

    Some(false)
}

/// Find the position of the closing `)` that ends the findIndex call,
/// handling nested parens.
fn find_callback_end(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_simple_find_index(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-array-index-of".into(),
                    message: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.".into(),
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
    fn flags_findindex_arrow_equality() {
        assert_eq!(run("const i = arr.findIndex(x => x === val);").len(), 1);
    }

    #[test]
    fn flags_findindex_parens_arrow() {
        assert_eq!(run("const i = arr.findIndex((x) => x === val);").len(), 1);
    }

    #[test]
    fn flags_findindex_reversed_comparison() {
        assert_eq!(run("const i = arr.findIndex(x => val === x);").len(), 1);
    }

    #[test]
    fn flags_findlastindex() {
        assert_eq!(
            run("const i = arr.findLastIndex(x => x === val);").len(),
            1
        );
    }

    #[test]
    fn allows_indexof() {
        assert!(run("const i = arr.indexOf(val);").is_empty());
    }

    #[test]
    fn allows_complex_callback() {
        assert!(run("const i = arr.findIndex(x => x.id === val);").is_empty());
    }
}
