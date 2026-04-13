//! no-magic-array-flat-depth AST backend — flag `.flat(N)` where N is a
//! magic number (not 1).

use crate::diagnostic::{Diagnostic, Severity};

/// Detect `.flat(N)` where N is a numeric literal that is NOT 1.
fn has_magic_flat_depth(line: &str) -> bool {
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

        if arg.is_empty() {
            search_from = after_flat + close;
            continue;
        }

        if arg == "Infinity" || arg == "Number.POSITIVE_INFINITY" {
            search_from = after_flat + close;
            continue;
        }

        if let Ok(val) = arg.parse::<f64>() {
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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_magic_flat_depth(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-magic-array-flat-depth".into(),
                message: "Magic number as `.flat()` depth is not allowed. Use a named constant or `Infinity`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_magic_number_depth() {
        assert_eq!(run_on("arr.flat(3);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_two() {
        assert_eq!(run_on("arr.flat(2);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_large() {
        assert_eq!(run_on("const result = items.flat(10);").len(), 1);
    }

    #[test]
    fn allows_flat_without_args() {
        assert!(run_on("arr.flat();").is_empty());
    }

    #[test]
    fn allows_flat_depth_one() {
        assert!(run_on("arr.flat(1);").is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        assert!(run_on("arr.flat(Infinity);").is_empty());
    }

    #[test]
    fn allows_flat_variable() {
        assert!(run_on("arr.flat(depth);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run_on("// arr.flat(3);").is_empty());
    }
}
