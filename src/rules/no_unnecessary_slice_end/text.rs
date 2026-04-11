use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.slice(expr, obj.length)` / `.slice(expr, Infinity)` /
/// `.slice(expr, Number.POSITIVE_INFINITY)`.
///
/// The second argument (end) is unnecessary when it means "to the end",
/// because omitting it already does that.
fn has_unnecessary_slice_end(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
        return false;
    }

    let mut search_from = 0;
    while let Some(pos) = trimmed[search_from..].find(".slice(") {
        let abs_pos = search_from + pos;
        let after_method = abs_pos + ".slice(".len();

        let rest = &trimmed[after_method..];
        let close = match rest.find(')') {
            Some(p) => p,
            None => {
                search_from = after_method;
                continue;
            }
        };

        let args_str = &rest[..close];
        // Must have exactly one comma (two arguments)
        let comma = match args_str.find(',') {
            Some(p) => p,
            None => {
                search_from = after_method + close;
                continue;
            }
        };

        // Check there's no second comma (slice only takes 2 args anyway)
        if args_str[comma + 1..].contains(',') {
            search_from = after_method + close;
            continue;
        }

        let second_arg = args_str[comma + 1..].trim();

        // Flag if the second arg is: Infinity, Number.POSITIVE_INFINITY,
        // or something.length
        if second_arg == "Infinity"
            || second_arg == "Number.POSITIVE_INFINITY"
            || second_arg.ends_with(".length")
        {
            return true;
        }

        search_from = after_method + close;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_unnecessary_slice_end(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unnecessary-slice-end".into(),
                    message: "The `end` argument is unnecessary \u{2014} `.slice(start)` already goes to the end.".into(),
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
    fn flags_slice_with_length() {
        assert_eq!(run("arr.slice(2, arr.length);").len(), 1);
    }

    #[test]
    fn flags_slice_with_infinity() {
        assert_eq!(run("str.slice(0, Infinity);").len(), 1);
    }

    #[test]
    fn flags_slice_with_number_positive_infinity() {
        assert_eq!(run("arr.slice(1, Number.POSITIVE_INFINITY);").len(), 1);
    }

    #[test]
    fn allows_slice_without_end() {
        assert!(run("arr.slice(2);").is_empty());
    }

    #[test]
    fn allows_slice_with_numeric_end() {
        assert!(run("arr.slice(2, 5);").is_empty());
    }

    #[test]
    fn allows_slice_with_variable_end() {
        assert!(run("arr.slice(0, end);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// arr.slice(2, arr.length);").is_empty());
    }
}
