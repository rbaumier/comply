use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.flat(1)` -- the default depth, which is unnecessary to specify.
fn has_unnecessary_flat_depth(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
        return false;
    }

    let mut search_from = 0;
    while let Some(pos) = trimmed[search_from..].find(".flat(") {
        let abs_pos = search_from + pos;
        let after_flat = abs_pos + ".flat(".len();

        let rest = &trimmed[after_flat..];
        let close = match rest.find(')') {
            Some(p) => p,
            None => {
                search_from = after_flat;
                continue;
            }
        };

        let arg = rest[..close].trim();
        if arg == "1" {
            return true;
        }

        search_from = after_flat + close;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_unnecessary_flat_depth(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unnecessary-array-flat-depth".into(),
                    message: "Passing `1` as the `depth` argument of `.flat()` is unnecessary \u{2014} it is the default.".into(),
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
    fn flags_flat_one() {
        assert_eq!(run("arr.flat(1);").len(), 1);
    }

    #[test]
    fn flags_flat_one_with_spaces() {
        assert_eq!(run("arr.flat( 1 );").len(), 1);
    }

    #[test]
    fn allows_flat_no_args() {
        assert!(run("arr.flat();").is_empty());
    }

    #[test]
    fn allows_flat_other_depth() {
        assert!(run("arr.flat(2);").is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        assert!(run("arr.flat(Infinity);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// arr.flat(1);").is_empty());
    }
}
