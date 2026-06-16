//! Port of Biome `noUnmatchableAnbSelector`.
//!
//! An An+B selector (`:nth-child`, `:nth-last-child`, `:nth-of-type`,
//! `:nth-last-of-type`) whose formula evaluates to the constant `0` can never
//! match an element. That is exactly the case where the coefficient `A` and the
//! offset `B` are both `0`: `:nth-child(0)`, `:nth-child(0n)`, `:nth-child(0n+0)`
//! and their sign/whitespace variants (`+0n`, `-0n`, `0n-0`). Any non-zero `A`
//! or `B` makes the formula match at least one element (`:nth-child(0n+1)`,
//! `:nth-child(2n)`), and the keywords `even`/`odd` always match.
//!
//! A flagged selector is exempt when nested in an odd number of `:not()`
//! pseudo-classes, since `:not()` inverts the match: `a:not(:nth-child(0))`
//! matches every element and is therefore valid.
//!
//! tree-sitter-css tokenizes the An+B argument inconsistently (`0n+0` is one
//! `plain_value`, while `0n-0` splits into a `plain_value` plus an `ERROR`
//! node), so the formula is recovered from the raw text inside the `arguments`
//! node and parsed directly rather than from child node kinds.

use crate::diagnostic::{Diagnostic, Severity};

/// The pseudo-classes that carry an An+B argument. Compared case-insensitively.
const NTH_PSEUDO_CLASSES: &[&str] = &[
    "nth-child",
    "nth-last-child",
    "nth-of-type",
    "nth-last-of-type",
];

/// Text of the first direct named child of `node` whose kind is `kind`.
fn child_text<'a>(node: &tree_sitter::Node, kind: &str, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
        .and_then(|child| child.utf8_text(source).ok())
}

/// Whether the formula in `args` (the raw text inside the `arguments` parens,
/// e.g. `0n+0`, `-0n`, `0 of a`, `even`) can never match an element.
///
/// Returns `false` for any formula that is dynamic (keywords, SCSS-style
/// interpolation) or that we cannot fully parse, matching Biome's conservative
/// "only flag a provably-zero constant" behavior.
fn is_unmatchable(args: &str) -> bool {
    // Drop the `of <selector>` part — it does not affect the An+B value.
    let formula = match args.to_ascii_lowercase().find(" of ") {
        Some(idx) => &args[..idx],
        None => args,
    };
    let formula = formula.trim();

    // Locate the `n` separating the coefficient from the offset.
    let Some(n_pos) = formula.bytes().position(|b| b == b'n' || b == b'N') else {
        // No `n`: a plain `B` constant. Unmatchable iff it is exactly zero.
        return parse_int(formula) == Some(0);
    };

    // Coefficient `A` is the text before `n`. An empty/`+`/`-` coefficient means
    // `±1`, which is non-zero, so only an explicit `0`/`+0`/`-0` is zero.
    let coeff_text = &formula[..n_pos];
    let a_is_zero = match coeff_text {
        "" | "+" | "-" => false,
        other => parse_int(other) == Some(0),
    };

    // Offset `B` is the text after `n`. Empty means no offset.
    let offset_text = formula[n_pos + 1..].trim();
    if offset_text.is_empty() {
        // `A == 0` with no offset is unmatchable (`:nth-child(0n)`).
        return a_is_zero;
    }
    let Some(b) = parse_signed_offset(offset_text) else {
        return false;
    };

    // Both terms present: unmatchable iff both are zero.
    a_is_zero && b == 0
}

/// Parse an integer that may carry a leading `+`/`-` and surrounding spaces.
/// Returns `None` for anything that is not a plain integer (e.g. `2n`, `foo`).
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim();
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Parse the offset term, where the sign may be detached from the digits by
/// whitespace (`+ 0`, `- 0`) as tree-sitter-css can surface it.
fn parse_signed_offset(text: &str) -> Option<i64> {
    let text = text.trim();
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let rest = rest.trim();
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse::<i64>().ok().map(|value| sign * value)
}

/// Number of `:not()` pseudo-classes the selector is nested in. `:not()` inverts
/// the match, so an odd count means an unmatchable formula actually matches
/// everything and must not be flagged.
fn enclosing_not_count(node: &tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        if ancestor.kind() == "pseudo_class_selector"
            && let Some(name) = child_text(&ancestor, "class_name", source)
            && name.eq_ignore_ascii_case("not")
        {
            count += 1;
        }
        cursor = ancestor.parent();
    }
    count
}

crate::ast_check! { on ["pseudo_class_selector"] prefilter = ["nth-"] => |node, source, ctx, diagnostics|
    let Some(class_name) = child_text(&node, "class_name", source) else {
        return;
    };
    if !NTH_PSEUDO_CLASSES
        .iter()
        .any(|name| class_name.eq_ignore_ascii_case(name))
    {
        return;
    }
    let Some(args) = child_text(&node, "arguments", source) else {
        return;
    };
    // `arguments` text includes the surrounding parens, e.g. `(0n+0)`.
    let inner = args
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(args);

    if is_unmatchable(inner) && enclosing_not_count(&node, source).is_multiple_of(2) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!(
                "This `:{class_name}` selector can never match an element because its An+B formula resolves to 0."
            ),
            Severity::Error,
        ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // --- Biome `invalid.css` fixtures: every line fires (one part each). ---

    #[test]
    fn invalid_fixtures_fire() {
        let cases = [
            "a:nth-child(0) {}",
            "a:nth-child(0n) {}",
            "a:nth-child(+0n) {}",
            "a:nth-child(-0n) {}",
            "a:nth-child(0n+0) {}",
            "a:nth-child(0n-0) {}",
            "a:nth-child(-0n-0) {}",
            "a:nth-child(0 of a) {}",
            "a:nth-last-child(0) {}",
            "a:nth-of-type(0) {}",
            "a:nth-last-of-type(0) {}",
        ];
        for src in cases {
            assert_eq!(run(src).len(), 1, "expected one diagnostic for {src}");
        }
    }

    #[test]
    fn invalid_compound_flags_only_zero_part() {
        // `a:nth-child(0), a:nth-child(1)` — only the `(0)` selector fires.
        assert_eq!(run("a:nth-child(0), a:nth-child(1) {}").len(), 1);
        // `a:nth-child(0n):nth-child(-n+5)` — only the inner `(0n)` fires.
        assert_eq!(run("a:nth-child(0n):nth-child(-n+5) {}").len(), 1);
        // `a:nth-last-child(0),a:nth-last-child(n+5) ~ li` — only `(0)` fires.
        assert_eq!(
            run("a:nth-last-child(0),a:nth-last-child(n+5) ~ li {}").len(),
            1
        );
    }

    // --- Biome `valid.css` fixtures: must not fire. ---

    #[test]
    fn valid_fixtures_do_not_fire() {
        let cases = [
            "a:nth-child(1) {}",
            "a:nth-child(2n) {}",
            "a:nth-child(0n+1) {}",
            "a:nth-child(0n-1) {}",
            "a:nth-child(2n+0) {}",
            "a:nth-child(2n+2) {}",
            "a:nth-child(2n-0) {}",
            "a:nth-child(1 of a) {}",
            "a:nth-last-child(1) {}",
            "a:nth-of-type(1) {}",
            "a:nth-last-of-type(1) {}",
            "a:nth-child(even) {}",
            "a:nth-child(odd) {}",
            "a:not(:nth-child(0)) {}",
            "a:nth-child(n+1):nth-child(-n+5) {}",
            "a:nth-last-child(n+5),a:nth-last-child(n+5) ~ li {}",
        ];
        for src in cases {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    // --- An+B parser edge cases. ---

    #[test]
    fn whitespace_around_formula_is_ignored() {
        // `0n + 0` with internal spaces is still the zero constant.
        assert_eq!(run("a:nth-child( 0n + 0 ) {}").len(), 1);
        // A non-zero offset with spaces is still matchable.
        assert!(run("a:nth-child( 0n + 1 ) {}").is_empty());
    }

    #[test]
    fn coefficient_is_case_insensitive_on_n() {
        // Uppercase `N` is the same coefficient token.
        assert_eq!(run("a:nth-child(0N+0) {}").len(), 1);
        assert_eq!(run("a:nth-child(0N) {}").len(), 1);
    }

    #[test]
    fn pseudo_class_name_is_case_insensitive() {
        assert_eq!(run("a:NTH-CHILD(0) {}").len(), 1);
    }

    #[test]
    fn double_not_is_not_exempt() {
        // Two `:not()` cancel out, so the inner zero formula is flagged again.
        assert_eq!(run("a:not(:not(:nth-child(0))) {}").len(), 1);
    }

    #[test]
    fn non_nth_pseudo_class_is_ignored() {
        // `:nth` is not a real An+B pseudo-class; other pseudo-classes too.
        assert!(run("a:hover {}").is_empty());
        assert!(run("a:first-child {}").is_empty());
    }

    #[test]
    fn matchable_formulas_with_zero_coefficient_only() {
        // `0n+5` matches the 5th element; not unmatchable.
        assert!(run("a:nth-child(0n+5) {}").is_empty());
    }
}
