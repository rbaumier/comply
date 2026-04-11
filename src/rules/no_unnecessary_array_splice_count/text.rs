use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.splice(expr, obj.length)` / `.splice(expr, Infinity)` /
/// `.splice(expr, Number.POSITIVE_INFINITY)` and same for `.toSpliced(...)`.
///
/// The second argument (deleteCount/skipCount) is unnecessary when it means
/// "everything from start to end", because omitting it already does that.
fn has_unnecessary_splice_count(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
        return false;
    }

    for method in &[".splice(", ".toSpliced("] {
        let mut search_from = 0;
        while let Some(pos) = trimmed[search_from..].find(method) {
            let abs_pos = search_from + pos;
            let after_method = abs_pos + method.len();

            let rest = &trimmed[after_method..];
            // Find closing paren (simple: doesn't handle nested parens, but
            // sufficient for the common `splice(start, count)` pattern)
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

            // Check there's no second comma (more than 2 args means replacement items)
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
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_unnecessary_splice_count(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unnecessary-array-splice-count".into(),
                    message: "The count argument is unnecessary \u{2014} `.splice(start)` already removes all elements from `start`.".into(),
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
    fn flags_splice_with_length() {
        assert_eq!(run("arr.splice(2, arr.length);").len(), 1);
    }

    #[test]
    fn flags_splice_with_infinity() {
        assert_eq!(run("arr.splice(0, Infinity);").len(), 1);
    }

    #[test]
    fn flags_splice_with_number_positive_infinity() {
        assert_eq!(run("arr.splice(1, Number.POSITIVE_INFINITY);").len(), 1);
    }

    #[test]
    fn flags_to_spliced_with_length() {
        assert_eq!(run("arr.toSpliced(2, arr.length);").len(), 1);
    }

    #[test]
    fn allows_splice_without_count() {
        assert!(run("arr.splice(2);").is_empty());
    }

    #[test]
    fn allows_splice_with_numeric_count() {
        assert!(run("arr.splice(2, 3);").is_empty());
    }

    #[test]
    fn allows_splice_with_replacement_items() {
        // 3+ args means the second arg is a real deleteCount with replacements
        assert!(run("arr.splice(2, arr.length, 'a', 'b');").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// arr.splice(2, arr.length);").is_empty());
    }
}
