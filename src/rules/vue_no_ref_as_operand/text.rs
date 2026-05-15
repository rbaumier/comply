//! vue-no-ref-as-operand text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

/// Collect identifiers bound to a `ref(...)` / `shallowRef(...)` /
/// `computed(...)` call in the source. Heuristic: look for
/// `const X = ref(...)` patterns.
fn collect_ref_bindings(source: &str) -> HashSet<String> {
    let mut bindings = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let after_kw = trimmed
            .strip_prefix("const ")
            .or_else(|| trimmed.strip_prefix("let "));
        let Some(rest) = after_kw else { continue };
        // Split on '=' to get the name.
        let Some((lhs, rhs)) = rest.split_once('=') else { continue };
        let name = lhs.split([':', ' ']).next().unwrap_or("").trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
            continue;
        }
        let rhs_trim = rhs.trim_start();
        if rhs_trim.starts_with("ref(")
            || rhs_trim.starts_with("shallowRef(")
            || rhs_trim.starts_with("customRef(")
            || rhs_trim.starts_with("computed(")
            || rhs_trim.starts_with("toRef(")
        {
            bindings.insert(name.to_string());
        }
    }
    bindings
}

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let bindings = collect_ref_bindings(ctx.source);
        if bindings.is_empty() {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        // Look for `<name> + ` / `<name> ===` / `<name>++` / `<name>--`
        // patterns where the binding is used like a primitive.
        for name in &bindings {
            for (i, _) in ctx.source.match_indices(name.as_str()) {
                // Word boundary on left.
                let prev_ok = i == 0
                    || ctx.source.as_bytes()[i - 1].is_ascii_whitespace()
                    || matches!(
                        ctx.source.as_bytes()[i - 1],
                        b'(' | b'[' | b'{' | b',' | b';' | b'=' | b'+' | b'-' | b'!'
                    );
                if !prev_ok {
                    continue;
                }
                let end = i + name.len();
                if end >= ctx.source.len() {
                    continue;
                }
                let after = &ctx.source[end..];
                let next_char = after.chars().next();
                let after_trim = after.trim_start();
                // Allow `.value`, `.something`, function-call, assignment.
                if after.starts_with('.') {
                    continue;
                }
                // Operators that misuse the ref as a primitive.
                let misuse = after_trim.starts_with("++")
                    || after_trim.starts_with("--")
                    || after_trim.starts_with("+ ")
                    || after_trim.starts_with("- ")
                    || after_trim.starts_with("* ")
                    || after_trim.starts_with("/ ")
                    || after_trim.starts_with("=== ")
                    || after_trim.starts_with("!== ")
                    || after_trim.starts_with("== ")
                    || after_trim.starts_with("!= ")
                    || (next_char == Some(' ')
                        && (after_trim.starts_with("+ ")
                            || after_trim.starts_with("- ")
                            || after_trim.starts_with("> ")
                            || after_trim.starts_with("< ")));
                if !misuse {
                    continue;
                }
                let (line, column) = byte_to_line_col(ctx.source, i);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is a ref — unwrap with `.value` before using it as \
                         an arithmetic/comparison operand."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_ref_arithmetic() {
        let src = "const count = ref(0);\nconst x = count + 1;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_value_arithmetic() {
        let src = "const count = ref(0);\nconst x = count.value + 1;";
        assert!(run(src).is_empty());
    }
}
