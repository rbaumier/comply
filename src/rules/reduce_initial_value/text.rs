use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `.reduce(callback)` without a second argument.
///
/// Strategy: find `.reduce(`, then walk forward counting parens.
/// When the outermost paren closes, check whether we saw a comma
/// at depth == 1 (the separator between callback and initial value).
fn has_reduce_without_initial(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".reduce(") {
        let abs = start + pos + 8; // skip past ".reduce("
        let mut depth: i32 = 1;
        let mut comma_at_depth1 = false;
        let bytes = line.as_bytes();
        let mut i = abs;

        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                b',' if depth == 1 => {
                    comma_at_depth1 = true;
                }
                _ => {}
            }
            i += 1;
        }

        if depth == 0 && !comma_at_depth1 {
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
            if has_reduce_without_initial(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "reduce-initial-value".into(),
                    message: "`.reduce()` without initial value — throws on empty arrays.".into(),
                    severity: Severity::Error,
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
    fn flags_reduce_without_initial() {
        assert_eq!(run("const sum = arr.reduce((acc, x) => acc + x);").len(), 1);
    }

    #[test]
    fn flags_reduce_with_arrow_body() {
        assert_eq!(run("const r = items.reduce((a, b) => a.concat(b));").len(), 1);
    }

    #[test]
    fn allows_reduce_with_initial_value() {
        assert!(run("const sum = arr.reduce((acc, x) => acc + x, 0);").is_empty());
    }

    #[test]
    fn allows_reduce_with_object_initial() {
        assert!(run("const m = arr.reduce((acc, x) => ({ ...acc, [x]: 1 }), {});").is_empty());
    }
}
