//! explicit-length-check — OXC backend.
//! Scans line-by-line for implicit `.length`/`.size` boolean coercion.
//! This rule is text-based (no AST nodes needed), so we use `run_on_semantic`
//! to iterate lines, matching the tree-sitter backend's behaviour.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns true if the `.length`/`.size` access starting at `prop_pos` in `trimmed`
/// is the direct argument of an `expect(...)` call (Vitest/Jest matcher form),
/// in which case the matcher (`.toBe`, `.toBeGreaterThan`, ...) performs the
/// explicit comparison and the rule should not flag.
fn is_inside_expect_argument(trimmed: &str, prop_pos: usize) -> bool {
    let bytes = trimmed.as_bytes();
    let mut i = prop_pos;
    let mut depth: i32 = 0;
    while i > 0 {
        i -= 1;
        let c = bytes[i];
        match c {
            b')' | b']' => depth += 1,
            b'(' | b'[' => {
                if depth == 0 {
                    if c == b'[' {
                        return false;
                    }
                    let mut j = i;
                    while j > 0
                        && (bytes[j - 1].is_ascii_alphanumeric()
                            || bytes[j - 1] == b'_'
                            || bytes[j - 1] == b'$')
                    {
                        j -= 1;
                    }
                    if &trimmed[j..i] == "expect" {
                        return true;
                    }
                    // Not `expect` — keep walking outward past this call.
                    // depth stays 0; continue the outer loop.
                } else {
                    depth -= 1;
                }
            }
            // Structural boundaries at depth 0 mean we left the expression.
            b',' | b';' | b'{' | b'}' if depth == 0 => return false,
            b'=' if depth == 0 => {
                // `==`, `!=`, `>=`, `<=` are comparisons, not assignments.
                if i + 1 < bytes.len()
                    && (bytes[i + 1] == b'=' || bytes[i + 1] == b'>')
                {
                    // comparison operator — not a boundary
                } else if i > 0
                    && (bytes[i - 1] == b'!' || bytes[i - 1] == b'<' || bytes[i - 1] == b'>')
                {
                    // part of !=, <=, >= — not a boundary
                } else {
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}

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

            // A trailing `,` always means a value position (argument, array
            // element, object property value, declarator) — never a boolean
            // coercion — so it is deliberately not a flagging context.
            if rest.is_empty()
                || rest.starts_with(')')
                || rest.starts_with("&&")
                || rest.starts_with("||")
                || rest.starts_with('?')
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

                if is_inside_expect_argument(trimmed, abs_pos) {
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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_implicit_length_check(line) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
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
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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

    #[test]
    fn allows_expect_to_be_greater_than() {
        assert!(run_on("expect(value.length).toBeGreaterThan(0);").is_empty());
    }

    #[test]
    fn allows_expect_to_be() {
        assert!(run_on("expect(arr.length).toBe(3);").is_empty());
    }

    #[test]
    fn allows_expect_to_equal() {
        assert!(run_on("expect(arr.length).toEqual(0);").is_empty());
    }

    #[test]
    fn allows_length_inside_nested_call_inside_expect() {
        // Real-world pattern: filter then check length non-empty.
        let src = "expect(arr.filter(x => x.foo).length).toBeGreaterThan(0);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_length_inside_object_keys_inside_expect() {
        let src = "expect(Object.keys(obj).length).toBe(3);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_length_in_unrelated_call() {
        // `notExpect` is a regular function — still flags.
        let src = "notExpect(arr.length);";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for #259: `.length` read as an object-property value is a
    // numeric use, not a boolean coercion.
    #[test]
    fn allows_length_as_object_property_value() {
        assert!(run_on("count: list.length,").is_empty());
    }
}
