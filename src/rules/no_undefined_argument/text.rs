use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check whether a line contains `undefined` inside function call parentheses.
/// Matches patterns like `foo(undefined)`, `foo(x, undefined, y)`, `foo(undefined, x)`.
fn has_undefined_arg(line: &str) -> bool {
    // Look for `(` ... `undefined` ... `)` where `undefined` appears as an argument.
    let mut start = 0;
    while let Some(paren_pos) = line[start..].find('(') {
        let abs = start + paren_pos + 1;
        if let Some(close) = line[abs..].find(')') {
            let args = &line[abs..abs + close];
            for arg in args.split(',') {
                if arg.trim() == "undefined" {
                    return true;
                }
            }
            start = abs + close + 1;
        } else {
            break;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_undefined_arg(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-undefined-argument".into(),
                    message: "Do not pass `undefined` as an argument — omit the argument instead."
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
    fn flags_sole_undefined_arg() {
        assert_eq!(run("foo(undefined);").len(), 1);
    }

    #[test]
    fn flags_undefined_among_args() {
        assert_eq!(run("foo(x, undefined, y);").len(), 1);
    }

    #[test]
    fn allows_no_undefined() {
        assert!(run("foo(x, y);").is_empty());
    }

    #[test]
    fn allows_undefined_in_variable_name() {
        assert!(run("foo(undefinedValue);").is_empty());
    }
}
