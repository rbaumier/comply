//! prefer-string-replace-all OXC backend — flag `.replace(/pattern/g, ...)`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, RegExpFlags};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".replace"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "replace" {
            return;
        }

        // First argument must be a regex literal with the `g` flag.
        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::RegExpLiteral(regex) = first_arg else { return };

        if !regex.regex.flags.contains(RegExpFlags::G) {
            return;
        }

        // `String#replaceAll(string)` can only replace a fixed literal substring.
        // A regex with anchors, alternation, quantifiers, classes, or assertions
        // is not equivalent to any constant string, so suggesting `.replaceAll`
        // would silently change behavior. Only flag fixed-literal patterns.
        if !regex_pattern_is_fixed_literal(regex.regex.pattern.text.as_str()) {
            return;
        }

        // Anchor at the `replace` property identifier. For a chained member call
        // (`s.replace(/a/g).replace(/b/g)`), oxc spans every `CallExpression` from
        // the chain root, so `call.span.start` would stack all diagnostics on the
        // leftmost object; `member.property.span.start` points at each `.replace`.
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `String#replaceAll()` over `String#replace()` with a global regex."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Whether a regex source pattern matches exactly one constant substring, so
/// `String#replace(/p/g, r)` can be rewritten as `String#replaceAll("p", r)`
/// without changing behavior.
///
/// Walks the source char-by-char. An unescaped regex metacharacter (anchor,
/// alternation, quantifier, group, or class delimiter) means the pattern is not
/// a fixed string. A backslash escapes the next char: escaping an ASCII
/// punctuation metacharacter (`\.`, `\+`, `\\`, `\/`, ...) yields that literal
/// punctuation char, but a backslash before a letter or digit introduces a class
/// shorthand (`\d`, `\w`, `\b`), an assertion, or a numeric/unicode escape
/// (`\0`, `\n`, `\xNN`, `\uNNNN`), none of which denote a fixed substring. A
/// dangling trailing backslash is treated as non-literal. When in doubt, return
/// false so the rule stays silent rather than risk a behavior-changing rewrite.
fn regex_pattern_is_fixed_literal(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                // Escaped ASCII punctuation denotes the literal punctuation char.
                Some(next) if next.is_ascii_punctuation() => {}
                // Class shorthand, assertion, numeric/unicode escape, or a
                // dangling backslash — not a fixed substring.
                _ => return false,
            }
        } else if matches!(
            c,
            '^' | '$' | '.' | '[' | ']' | '(' | ')' | '|' | '+' | '*' | '?' | '{' | '}'
        ) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("prefer-string-replace-all", source, "t.ts")
    }

    #[test]
    fn flags_replace_with_global_regex() {
        let d = run(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-replace-all");
        // Anchored at `replace` (column 5), not the `str` chain root (column 1).
        assert_eq!((d[0].line, d[0].column), (1, 5));
    }

    #[test]
    fn allows_replace_without_global() {
        assert!(run(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run(r#"str.replace('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_all_already() {
        assert!(run(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }

    // Regression for #3818: a chained `.replace().replace()` must emit one
    // diagnostic per `.replace`, each anchored at its own `replace` method, not
    // all stacked on the chain-root identifier. oxc spans every CallExpression
    // in the chain from the leftmost object, so anchoring at `call.span.start`
    // collapsed every link onto the same column.
    #[test]
    fn chained_replace_anchors_each_link_at_its_own_method() {
        let source = "export function f(s: string) {\n  return s.replace(/#/g, \"%23\").replace(/\\?/g, \"%3F\");\n}";
        let d = run(source);
        assert_eq!(d.len(), 2, "one diagnostic per global-regex .replace");

        // Both links are on line 2; the chain root `s` is at column 10.
        assert_eq!(d[0].line, 2);
        assert_eq!(d[1].line, 2);

        // The two `replace` methods sit at distinct columns: `  return s.` is
        // 11 chars so the first `replace` starts at column 12; the second follows
        // `.replace(/#/g, "%23").` and starts at column 33. (Emission order follows
        // AST traversal — outer call first — so compare as a sorted set.)
        let mut columns: Vec<usize> = d.iter().map(|diag| diag.column).collect();
        columns.sort_unstable();
        assert_eq!(columns, vec![12, 33]);

        // Neither diagnostic is anchored at the chain-root token `s` (column 10).
        assert!(columns.iter().all(|&c| c != 10));
    }

    // Regression for #6662: a global regex whose pattern is not equivalent to a
    // fixed literal must not be flagged — `.replaceAll(string)` would silently
    // change behavior.
    #[test]
    fn allows_anchors_and_alternation() {
        // `/^"|"$/g` — anchors (`^`, `$`) plus alternation (`|`).
        assert!(run(r#"str.replace(/^"|"$/g, "")"#).is_empty());
    }

    #[test]
    fn allows_quantifier() {
        // `/\\+/g` — one-or-more backslashes (`+` quantifier), not a fixed string.
        assert!(run(r"str.replace(/\\+/g, 'x')").is_empty());
    }

    #[test]
    fn allows_character_class() {
        assert!(run(r"str.replace(/[ab]/g, 'x')").is_empty());
    }

    #[test]
    fn allows_class_shorthand() {
        assert!(run(r"str.replace(/\d/g, 'x')").is_empty());
    }

    #[test]
    fn flags_escaped_punctuation_literal() {
        // `/\./g` matches a literal dot — a fixed substring, still convertible.
        let d = run(r"str.replace(/\./g, 'x')");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_quote_literal() {
        // `/\\"/g` matches the fixed two-char string `\"` — still convertible.
        let d = run(r#"str.replace(/\\"/g, '"')"#);
        assert_eq!(d.len(), 1);
    }
}
