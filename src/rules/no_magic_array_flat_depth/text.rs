use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.flat(N)` where N is a numeric literal that is NOT 1.
/// `.flat(1)` is handled by `no-unnecessary-array-flat-depth`.
/// `.flat()`, `.flat(Infinity)`, and `.flat(someVar)` are fine.
fn has_magic_flat_depth(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
        return false;
    }

    let mut search_from = 0;
    while let Some(pos) = trimmed[search_from..].find(".flat(") {
        let abs_pos = search_from + pos;
        let after_flat = abs_pos + ".flat(".len();

        // Find the closing paren
        let rest = &trimmed[after_flat..];
        let close = match rest.find(')') {
            Some(p) => p,
            None => {
                search_from = after_flat;
                continue;
            }
        };

        let arg = rest[..close].trim();

        // Empty arg -> `.flat()` is fine
        if arg.is_empty() {
            search_from = after_flat + close;
            continue;
        }

        // Skip non-numeric arguments (identifiers like `depth`, `Infinity`)
        if arg == "Infinity" || arg == "Number.POSITIVE_INFINITY" {
            search_from = after_flat + close;
            continue;
        }

        // Check if the argument is a numeric literal
        if let Ok(val) = arg.parse::<f64>() {
            // `.flat(1)` is not a magic number (handled by another rule)
            if (val - 1.0).abs() < f64::EPSILON {
                search_from = after_flat + close;
                continue;
            }
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
            if has_magic_flat_depth(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-magic-array-flat-depth".into(),
                    message: "Magic number as `.flat()` depth is not allowed. Use a named constant or `Infinity`.".into(),
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
    fn flags_magic_number_depth() {
        assert_eq!(run("arr.flat(3);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_two() {
        assert_eq!(run("arr.flat(2);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_large() {
        assert_eq!(run("const result = items.flat(10);").len(), 1);
    }

    #[test]
    fn allows_flat_without_args() {
        assert!(run("arr.flat();").is_empty());
    }

    #[test]
    fn allows_flat_depth_one() {
        assert!(run("arr.flat(1);").is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        assert!(run("arr.flat(Infinity);").is_empty());
    }

    #[test]
    fn allows_flat_variable() {
        assert!(run("arr.flat(depth);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// arr.flat(3);").is_empty());
    }
}
