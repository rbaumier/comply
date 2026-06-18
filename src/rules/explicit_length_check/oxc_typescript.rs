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

/// True when the `.length`/`.size` whose `.` is at `prop_pos` is consumed as a
/// numeric value rather than coerced to boolean. Shapes:
///   * right-hand operand of a comparison — `found.length !== other.length`
///     (`prefix.length` reached after a `===`/`!==`/`<`/`>`/`<=`/`>=`);
///   * a non-leading call/bracket argument — `slice(0, prefix.length)`
///     (the base is preceded by a `,` at the same nesting level).
///   * a ternary branch — `cond ? arr.length : 0` / `cond ? 0 : arr.length`
///     (the base is preceded by `?` or `:`), or an object-property value
///     (`{ k: arr.length }`); both are value positions, never boolean coercion.
///   * the right operand of a binary arithmetic expression — `30 + name.length`,
///     `total / sizes.length`, `(i + 1) % options.length` (the base is preceded
///     by `+`/`-`/`*`/`/`/`%`); arithmetic produces a number, so `.length` is a
///     numeric operand, never a boolean coercion.
/// A plain `=` assignment RHS (`x = arr.length`) is also a value position.
fn length_is_numeric_operand(trimmed: &str, prop_pos: usize) -> bool {
    let bytes = trimmed.as_bytes();
    // Walk left over the base expression (identifier / member / bracket / call
    // chain), balancing any `]`/`)` we pass through.
    let mut i = prop_pos;
    let mut depth: i32 = 0;
    while i > 0 {
        let c = bytes[i - 1];
        if depth > 0 {
            match c {
                b')' | b']' => depth += 1,
                b'(' | b'[' => depth -= 1,
                _ => {}
            }
            i -= 1;
            continue;
        }
        match c {
            b')' | b']' => {
                depth += 1;
                i -= 1;
            }
            _ if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'.' => {
                i -= 1;
            }
            _ => break,
        }
    }
    // Skip whitespace immediately to the left of the base expression.
    let mut j = i;
    while j > 0 && bytes[j - 1] == b' ' {
        j -= 1;
    }
    if j == 0 {
        return false;
    }
    // Comparison RHS (`===`, `!==`, `==`, `<`, `>`, `<=`, `>=`) or `=` assignment
    // RHS, a non-leading argument separated by `,`, a ternary branch /
    // object-property value preceded by `?`/`:`, or the right operand of a
    // binary arithmetic operator (`+`, `-`, `*`, `/`, `%`).
    match bytes[j - 1] {
        b'=' | b'<' | b'>' | b',' | b':' | b'+' | b'-' | b'*' | b'/' | b'%' => true,
        // `?` is a ternary delimiter only when not part of optional chaining:
        // `obj?.length` puts a `?` immediately left of the base chain too, but
        // there it is followed by `.` (`?.`) and the access stays a boolean
        // coercion. A ternary `?` is followed by whitespace/the value.
        b'?' => bytes[j] != b'.',
        _ => false,
    }
}

/// True when the `.length`/`.size` whose `.` is at `prop_pos` is the value of a
/// template-literal interpolation `${ <base>.length }` — a numeric value being
/// string-formatted, not a boolean coercion. Walks left over the base expression
/// (balancing `]`/`)`), skips whitespace, and checks the base is immediately
/// preceded by `${`.
fn is_template_interpolation_value(trimmed: &str, prop_pos: usize) -> bool {
    let bytes = trimmed.as_bytes();
    // Walk left over the base expression (identifier/member/bracket/call chain),
    // balancing brackets — mirror the loop in `length_is_numeric_operand`.
    let mut i = prop_pos;
    let mut depth: i32 = 0;
    while i > 0 {
        let c = bytes[i - 1];
        if depth > 0 {
            match c {
                b')' | b']' => depth += 1,
                b'(' | b'[' => depth -= 1,
                _ => {}
            }
            i -= 1;
            continue;
        }
        match c {
            b')' | b']' => {
                depth += 1;
                i -= 1;
            }
            _ if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'.' => {
                i -= 1;
            }
            _ => break,
        }
    }
    // Skip whitespace to the left of the base.
    let mut j = i;
    while j > 0 && bytes[j - 1] == b' ' {
        j -= 1;
    }
    // The base must be immediately preceded by `${`.
    j >= 2 && bytes[j - 1] == b'{' && bytes[j - 2] == b'$'
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

                // `${ base.length }` — the `.length` is the entire interpolated
                // value (closed by `}`), a numeric value being string-formatted,
                // not a boolean coercion. A ternary condition inside an
                // interpolation (`${arr.length ? a : b}`) is followed by `?`, not
                // `}`, so it stays a genuine coercion and still flags.
                if rest.starts_with('}') && is_template_interpolation_value(trimmed, abs_pos) {
                    search_from = after_prop;
                    continue;
                }

                if is_inside_expect_argument(trimmed, abs_pos) {
                    search_from = after_prop;
                    continue;
                }

                if length_is_numeric_operand(trimmed, abs_pos) {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    // Regression #589 — `.length` as a numeric `slice` argument, not boolean.
    #[test]
    fn allows_length_as_slice_argument_issue_589() {
        assert!(run_on("const head = full.slice(0, prefix.length);").is_empty());
    }

    // Regression #589 — comparing two lengths is already an explicit check.
    #[test]
    fn allows_two_length_comparison_issue_589() {
        assert!(run_on("if (found.length !== uniqueTeamIds.length) {}").is_empty());
    }

    #[test]
    fn allows_two_length_comparison_in_return_issue_589() {
        assert!(run_on("return found.length === expected.length;").is_empty());
    }

    // The boolean-coercion cases the rule exists for must still flag.
    #[test]
    fn still_flags_bare_length_in_boolean_call_issue_589() {
        assert_eq!(run_on("if (Boolean(arr.length)) {}").len(), 1);
    }

    // Regression #3914 — a ternary branch is a numeric value position, not a
    // boolean coercion. The consequent / alternate can sit on its own line.
    #[test]
    fn allows_length_as_ternary_consequent_issue_3914() {
        assert!(run_on("const n = cond ? obj.length : 0;").is_empty());
    }

    #[test]
    fn allows_length_as_ternary_alternate_issue_3914() {
        assert!(run_on("const n = cond ? 0 : obj.length;").is_empty());
    }

    #[test]
    fn allows_length_as_split_ternary_consequent_issue_3914() {
        // prettier src/language-yaml/utilities.js:221 shape.
        let src = "x = matches\n  ? matches.groups.leadingSpace.length\n  : Number.POSITIVE_INFINITY;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_length_as_split_ternary_alternate_issue_3914() {
        let src = "x = cond\n  ? 0\n  : obj.length;";
        assert!(run_on(src).is_empty());
    }

    // `:` also marks an object-property value — a value position, not coercion.
    #[test]
    fn allows_length_as_object_property_value_brace_issue_3914() {
        assert!(run_on("{ k: arr.length }").is_empty());
    }

    // Optional chaining in a boolean test MUST STILL FLAG: the `?` allow-list
    // addition is gated on the next char not being `.`, so `obj?.length`'s
    // optional-chaining `?` is not mistaken for a ternary delimiter.
    #[test]
    fn still_flags_optional_chain_length_in_if_issue_3914() {
        assert_eq!(run_on("if (a?.b.length) {}").len(), 1);
    }

    #[test]
    fn still_flags_optional_chain_length_as_call_arg_issue_3914() {
        assert_eq!(run_on("notExpect(a?.b.length);").len(), 1);
    }

    // Regression #3785 — `.length`/`.size` as the value of a template-literal
    // interpolation `${...}` is a numeric value being string-formatted, not a
    // boolean coercion.
    #[test]
    fn allows_length_as_template_interpolation_value_issue_3785() {
        assert!(run_on("console.log(`count: ${items.length}`);").is_empty());
    }

    #[test]
    fn allows_length_as_template_interpolation_value_with_whitespace_issue_3785() {
        assert!(run_on("`${ items.length }`").is_empty());
    }

    #[test]
    fn allows_size_as_template_interpolation_value_issue_3785() {
        assert!(run_on("`set: ${mySet.size}`").is_empty());
    }

    #[test]
    fn allows_member_base_length_as_template_interpolation_value_issue_3785() {
        assert!(run_on("`${obj.items.length}`").is_empty());
    }

    // A ternary condition inside an interpolation IS a genuine boolean coercion
    // (`.length` is followed by `?`, not `}`) and must still flag.
    #[test]
    fn still_flags_length_as_ternary_condition_in_interpolation_issue_3785() {
        assert_eq!(run_on("`${arr.length ? 'a' : 'b'}`").len(), 1);
    }

    // Regression #3788 — `.length`/`.size` as the right operand of a binary
    // arithmetic expression is a numeric operand, not a boolean coercion.
    #[test]
    fn allows_length_after_addition_issue_3788() {
        assert!(run_on("const buf = Buffer.allocUnsafe(30 + nameBytes.length);").is_empty());
    }

    #[test]
    fn allows_length_after_modulo_issue_3788() {
        assert!(run_on("next = (next + 1) % options.length;").is_empty());
    }

    #[test]
    fn allows_length_after_division_issue_3788() {
        assert!(run_on("const avg = total / sizes.length;").is_empty());
    }

    #[test]
    fn allows_length_after_multiplication_issue_3788() {
        assert!(run_on("const x = a * arr.length;").is_empty());
    }

    #[test]
    fn allows_length_after_subtraction_issue_3788() {
        assert!(run_on("const y = a - arr.length;").is_empty());
    }

    #[test]
    fn allows_size_after_addition_issue_3788() {
        assert!(run_on("const z = 5 + mySet.size;").is_empty());
    }
}
