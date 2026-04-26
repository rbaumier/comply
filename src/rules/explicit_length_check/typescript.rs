//! explicit-length-check backend — flag implicit `.length`/`.size` boolean coercion.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Check if a line has a bare `.length`/`.size` in a boolean context
/// (no explicit comparison like `> 0`, `=== 0`, `!== 0`, etc.).
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

            if after_prop < trimmed.len() {
                let after_char = trimmed.as_bytes()[after_prop];
                if after_char.is_ascii_alphanumeric() || after_char == b'_' {
                    search_from = after_prop;
                    continue;
                }
            }

            let rest = trimmed[after_prop..].trim_start();

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
                if (trimmed.starts_with("return ") || trimmed.starts_with("yield "))
                    && (rest.starts_with(';') || rest.is_empty())
                {
                    search_from = after_prop;
                    continue;
                }

                if (trimmed.starts_with("const ")
                    || trimmed.starts_with("let ")
                    || trimmed.starts_with("var "))
                    && (rest.starts_with(';') || rest.is_empty())
                {
                    search_from = after_prop;
                    continue;
                }

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

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_implicit_length_check(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "explicit-length-check".into(),
                    message: "Use explicit length comparison: `arr.length > 0` instead of \
                              `arr.length`, or `arr.length === 0` instead of `!arr.length`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_length_in_if() {
        assert_eq!(run_on("if (arr.length) {}").len(), 1);
    }

    #[test]
    fn flags_negated_length_in_if() {
        assert_eq!(run_on("if (!arr.length) {}").len(), 1);
    }

    #[test]
    fn allows_explicit_greater_than_zero() {
        assert!(run_on("if (arr.length > 0) {}").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run_on("const len = arr.length;").is_empty());
    }
}
