use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line has a bare `.length`/`.size` in a boolean context
/// (no explicit comparison like `> 0`, `=== 0`, `!== 0`, etc.).
///
/// Flags:
///   if (arr.length)       -> should be `if (arr.length > 0)`
///   if (!arr.length)      -> should be `if (arr.length === 0)`
///   while (arr.length)    -> should be `while (arr.length > 0)`
///   arr.length && ...     -> should be `arr.length > 0 && ...`
///   arr.length ? a : b    -> should be `arr.length > 0 ? a : b`
///
/// Does NOT flag:
///   if (arr.length > 0)   -> explicit
///   if (arr.length === 0) -> explicit
///   const x = arr.length  -> assignment
///   arr.length + 1        -> arithmetic
fn has_implicit_length_check(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
        return false;
    }

    for prop in &[".length", ".size"] {
        let mut search_from = 0;
        while let Some(pos) = trimmed[search_from..].find(prop) {
            let abs_pos = search_from + pos;
            let after_prop = abs_pos + prop.len();

            // Check the character before: must be an identifier char (part of the object)
            if abs_pos == 0 {
                search_from = after_prop;
                continue;
            }
            let before_char = trimmed.as_bytes()[abs_pos - 1];
            if !before_char.is_ascii_alphanumeric()
                && before_char != b'_'
                && before_char != b'$'
                && before_char != b']'
                && before_char != b')'
            {
                search_from = after_prop;
                continue;
            }

            // Check the character after: must NOT continue as an identifier
            // (e.g. `.lengthy` should not match)
            if after_prop < trimmed.len() {
                let after_char = trimmed.as_bytes()[after_prop];
                if after_char.is_ascii_alphanumeric() || after_char == b'_' {
                    search_from = after_prop;
                    continue;
                }
            }

            // Get what comes after the property (skip whitespace)
            let rest = trimmed[after_prop..].trim_start();

            // If followed by a comparison operator, it's explicit -- skip
            if rest.starts_with("> ")
                || rest.starts_with(">= ")
                || rest.starts_with("< ")
                || rest.starts_with("<= ")
                || rest.starts_with("=== ")
                || rest.starts_with("== ")
                || rest.starts_with("!== ")
                || rest.starts_with("!= ")
            {
                search_from = after_prop;
                continue;
            }

            // If followed by arithmetic, assignment, indexing, or chaining -- not boolean
            if rest.starts_with('+')
                || rest.starts_with('-')
                || rest.starts_with('*')
                || rest.starts_with('/')
                || rest.starts_with('%')
                || rest.starts_with('=')
                || rest.starts_with('[')
                || rest.starts_with('.')
            {
                search_from = after_prop;
                continue;
            }

            // Check it's in a boolean context: rest starts with ), &&, ||, ?, ,, ;, }, ]
            if rest.is_empty()
                || rest.starts_with(')')
                || rest.starts_with("&&")
                || rest.starts_with("||")
                || rest.starts_with('?')
                || rest.starts_with(',')
                || rest.starts_with(';')
                || rest.starts_with('}')
                || rest.starts_with(']')
            {
                // Exclude `return x.length;` / `yield x.length;` where
                // `.length` is the final value (followed by `;` or end).
                // But do NOT skip when `.length` is in a ternary/logical
                // inside a declaration, e.g. `const x = arr.length ? a : b`.
                if (trimmed.starts_with("return ") || trimmed.starts_with("yield "))
                    && (rest.starts_with(';') || rest.is_empty())
                {
                    search_from = after_prop;
                    continue;
                }

                // Exclude `const len = arr.length;` — plain assignment of
                // the length value (not a boolean test). Only skip when the
                // `.length` is followed by `;` (direct assignment), not when
                // it's followed by `?`, `&&`, `||` etc. (boolean context).
                if (trimmed.starts_with("const ")
                    || trimmed.starts_with("let ")
                    || trimmed.starts_with("var "))
                    && (rest.starts_with(';') || rest.is_empty())
                {
                    search_from = after_prop;
                    continue;
                }

                // If preceded by `= ` (assignment), skip (but not `==` or `!=`)
                let before = trimmed[..abs_pos].trim_end();
                if before.ends_with('=')
                    && !before.ends_with("==")
                    && !before.ends_with("!=")
                    && !before.ends_with(">=")
                    && !before.ends_with("<=")
                {
                    search_from = after_prop;
                    continue;
                }

                return true;
            }

            search_from = after_prop;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_implicit_length_check(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "explicit-length-check".into(),
                    message: "Use explicit length comparison: `arr.length > 0` instead of `arr.length`, or `arr.length === 0` instead of `!arr.length`.".into(),
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
    fn flags_bare_length_in_if() {
        assert_eq!(run("if (arr.length) {}").len(), 1);
    }

    #[test]
    fn flags_negated_length_in_if() {
        assert_eq!(run("if (!arr.length) {}").len(), 1);
    }

    #[test]
    fn flags_bare_size_in_if() {
        assert_eq!(run("if (map.size) {}").len(), 1);
    }

    #[test]
    fn flags_length_in_while() {
        assert_eq!(run("while (arr.length) {}").len(), 1);
    }

    #[test]
    fn flags_length_in_ternary() {
        assert_eq!(run("const x = arr.length ? 'yes' : 'no';").len(), 1);
    }

    #[test]
    fn flags_length_in_logical_and() {
        assert_eq!(run("arr.length && doSomething();").len(), 1);
    }

    #[test]
    fn allows_explicit_greater_than_zero() {
        assert!(run("if (arr.length > 0) {}").is_empty());
    }

    #[test]
    fn allows_explicit_triple_equals_zero() {
        assert!(run("if (arr.length === 0) {}").is_empty());
    }

    #[test]
    fn allows_explicit_not_equals_zero() {
        assert!(run("if (arr.length !== 0) {}").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run("const len = arr.length;").is_empty());
    }

    #[test]
    fn allows_arithmetic() {
        assert!(run("const x = arr.length + 1;").is_empty());
    }

    #[test]
    fn allows_return_statement() {
        assert!(run("return arr.length;").is_empty());
    }
}
