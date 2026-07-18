// TextCheck is appropriate here: Vue template directives are HTML-like syntax,
// not parseable by tree-sitter-typescript. The engine returns None for Vue SFCs
// (see engine.rs), so TreeSitter backends are skipped entirely for .vue files.

//! vue-no-duplicate-v-if text backend.
//!
//! Flags a `v-if="X"` directive that is immediately followed in source order
//! by `v-if="!X"` at the same indentation level — a text approximation of two
//! adjacent sibling elements at the same DOM depth, where the pattern signals
//! "should be v-if/v-else": two separate v-if directives evaluate
//! independently and can both render or both hide if timing is unlucky.
//!
//! A pair is flagged only when the two elements are genuine adjacent siblings
//! at the same nesting depth. Beyond equal indentation, this requires that
//! nothing at that indentation sits between them — an intervening always-
//! rendered element, or a `v-else`/`v-else-if` that already closes the base
//! chain, is itself an opening tag at that indent and breaks adjacency — and
//! that the negated element carries no slot directive (a named slot cannot
//! join a `v-else` chain). Different-indentation pairs are different nesting
//! depths and are skipped too. In each of these cases a `v-else` rewrite is
//! impossible or semantically wrong.
//!
//! Directives inside `<!-- ... -->` HTML comments are ignored: the source
//! is masked before scanning, so commented-out markup never pairs with live
//! markup.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// A `v-if` occurrence in source order: its 1-based line, condition, and the
/// leading-whitespace width (chars) of the original line.
struct VIf {
    line: usize,
    cond: String,
    indent: usize,
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let masked = crate::rules::vue_template_helpers::mask_html_comments(ctx.source);
        let lines: Vec<&str> = masked.lines().collect();

        // Collect v-if occurrences in source order, recording indentation so a
        // pair can be checked for adjacency at the same nesting depth.
        let mut occurrences: Vec<VIf> = Vec::new();
        for (idx, &line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if let Some(cond) = extract_v_if_condition(trimmed) {
                occurrences.push(VIf {
                    line: idx + 1,
                    cond: cond.to_string(),
                    indent: leading_whitespace_width(line),
                });
            }
        }

        // Flag only genuine adjacent-sibling opposite v-ifs: `v-if="X"` then
        // `v-if="!X"` at the same indent, with nothing at that indent between
        // them and no slot directive on the negated element — the only shape a
        // `v-else` could replace.
        let mut diagnostics = Vec::new();
        for pair in occurrences.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if a.indent == b.indent
                && is_negation_of(&b.cond, &a.cond)
                && !has_intervening_sibling(&lines, a, b)
                && !carries_slot_directive(lines[b.line - 1])
            {
                // Report the negated form (`v-if="!X"`), citing the `v-if="X"` line.
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: b.line,
                    column: 1,
                    rule_id: "vue-no-duplicate-v-if".into(),
                    message: format!(
                        "`v-if=\"{neg}\"` is the negation of `v-if=\"{base}\"` \
                         at line {}. Use `v-else` instead — two separate `v-if` \
                         directives evaluate independently.",
                        a.line,
                        neg = b.cond,
                        base = a.cond,
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Count the leading-whitespace characters of `line` (spaces and tabs alike).
/// Char-based so multibyte content after the whitespace is never sliced.
fn leading_whitespace_width(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Whether `cond` is exactly the boolean negation of `base` — one is the other
/// prefixed with a single `!` that negates the *entire* expression. A leading
/// `!` binds tighter than `&&`/`||`, so `!X && Y` parses as `(!X) && Y` and is
/// not the negation of `X && Y`; such pairs are rejected.
fn is_negation_of(cond: &str, base: &str) -> bool {
    is_full_negation(cond, base) || is_full_negation(base, cond)
}

/// Whether `negated` is `base` with a single leading `!` that negates the whole
/// expression. The `!` negates only the first operand when a top-level `&&`/`||`
/// follows (`!X && Y` = `(!X) && Y`), so the stripped remainder is required to
/// hold no top-level logical operator. A fully-parenthesized `!(X && Y)` keeps
/// its `&&` inside the parens (depth 1), so it still matches.
fn is_full_negation(negated: &str, base: &str) -> bool {
    match negated.strip_prefix('!') {
        Some(rest) => rest == base && !has_top_level_logical_operator(rest),
        None => false,
    }
}

/// Whether `s` contains a `&&` or `||` at bracket depth 0 — outside every
/// `()`/`[]`/`{}` group and outside string/template literals. Scanning is
/// quote-aware (`"`, `'`, `` ` ``, with `\` escapes) so an operator inside a
/// string value (`x === 'a && b'`) or a call argument (`fn(a && b)`) is not
/// counted as top-level.
fn has_top_level_logical_operator(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut in_str: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(quote) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == quote {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' | b'`' => in_str = Some(c),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'&' | b'|' if depth == 0 && bytes.get(i + 1) == Some(&c) => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Extract the condition from `v-if="..."`. Returns `None` if the line
/// doesn't contain a v-if directive.
fn extract_v_if_condition(line: &str) -> Option<&str> {
    let marker = "v-if=\"";
    let pos = line.find(marker)?;
    let start = pos + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Whether an element opening tag at the pair's indentation appears on a line
/// strictly between the two `v-if`s. Such a tag is an intervening sibling —
/// either an always-rendered element or a `v-else`/`v-else-if` that already
/// closes the base chain — so the two `v-if`s are not adjacent siblings and no
/// `v-else` rewrite is possible.
fn has_intervening_sibling(lines: &[&str], a: &VIf, b: &VIf) -> bool {
    // 0-based indices between the two 1-based occurrence lines: `a.line` is the
    // line just after `a`, `b.line - 1` excludes `b`'s own line.
    lines
        .get(a.line..b.line - 1)
        .unwrap_or(&[])
        .iter()
        .any(|line| leading_whitespace_width(line) == a.indent && is_element_open_tag(line.trim()))
}

/// Whether a trimmed line begins an element opening (or self-closing) tag such
/// as `<div ...>` or `<MyComp />`. Close tags (`</div>`), comments (`<!-- -->`),
/// and text/interpolation lines are excluded.
fn is_element_open_tag(trimmed: &str) -> bool {
    trimmed
        .strip_prefix('<')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// Whether the opening-tag line carries a Vue slot directive (`#name`,
/// `v-slot`, or `v-slot:name`). A slotted element targets a distinct named
/// slot, so it can never be the `v-else` branch of a preceding sibling.
///
/// Scans attribute-name positions of the first opening tag on the line and
/// stops at the tag's terminating `>`, skipping quoted values so a `#` or `>`
/// inside a value is never misread as a directive or terminator.
fn carries_slot_directive(line: &str) -> bool {
    let Some(after_lt) = line.trim_start().strip_prefix('<') else {
        return false;
    };
    let bytes = after_lt.as_bytes();
    let mut i = 0;
    // Skip the tag name.
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
        i += 1;
    }
    while i < bytes.len() && bytes[i] != b'>' {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            // Skip a quoted attribute value.
            i += 1;
            while i < bytes.len() && bytes[i] != b {
                i += 1;
            }
            i += 1;
        } else if b.is_ascii_whitespace() || b == b'/' || b == b'=' {
            i += 1;
        } else {
            let start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'='
                && bytes[i] != b'>'
                && bytes[i] != b'/'
            {
                i += 1;
            }
            let name = &after_lt[start..i];
            if name == "v-slot" || name.starts_with("v-slot:") || name.starts_with('#') {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source))
    }

    #[test]
    fn flags_opposite_v_ifs() {
        let source = "<div v-if=\"show\">A</div>\n<div v-if=\"!show\">B</div>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_v_if_v_else() {
        let source = "<div v-if=\"show\">A</div>\n<div v-else>B</div>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_unrelated_v_ifs() {
        let source = "<div v-if=\"a\">A</div>\n<div v-if=\"b\">B</div>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_commented_out_v_if() {
        // Regression #4424 (nuxt/devtools): a live `v-if="!x"` + `v-else` must
        // not pair with a `v-if="x"` that lives inside an HTML comment.
        let source = "<NButton v-if=\"!metrics.options.enabled\">Start</NButton>\n\
            <NButton v-else>Stop</NButton>\n\
            <!-- <template v-if=\"metrics.options.enabled\">\n\
            <NCheckbox />\n\
            </template> -->";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_live_duplicate_with_comment_present() {
        // Two LIVE opposite `v-if`s still flag even when a comment is nearby.
        let source = "<X v-if=\"a\" />\n<Y v-if=\"!a\" />";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_non_consecutive_opposite_v_ifs() {
        // Regression #4426 shape 1 (primevue AutoComplete): `!multiple` and
        // `multiple` are at the same indent but another `v-if`
        // (`isClearIconVisible`) sits between them, so they are not adjacent
        // siblings — a `v-else` rewrite is impossible.
        let source = "  <InputText v-if=\"!multiple\" />\n\
            \x20 <slot v-if=\"isClearIconVisible\" name=\"clearicon\" />\n\
            \x20 <ul v-if=\"multiple\" />";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_opposite_v_ifs_at_different_depths() {
        // Regression #4426 shape 2 (nuxt/devtools server-tasks): `v-if="selected"`
        // is nested inside a wrapper (deeper indent) and `v-if="!selected"` is a
        // shallower sibling — `v-else` cannot cross the wrapper close tag.
        let source = "  <KeepAlive>\n\
            \x20   <ServerTaskDetails v-if=\"selected\" />\n\
            \x20 </KeepAlive>\n\
            \x20 <NPanelGrids v-if=\"!selected\" />";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_opposite_v_ifs_nested_in_else_subtree() {
        // Regression #4426 shape 3 (nuxt/devtools overview): `v-if="!config"` at
        // one depth and `v-if="config"` nested deeper inside its else subtree.
        let source = "  <div v-if=\"!config\">\n\
            \x20   <Panel v-if=\"config\" />\n\
            \x20 </div>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_consecutive_same_indent_opposite_v_ifs() {
        // Load-bearing guard: two adjacent siblings at the same indentation that
        // are exact negations must still flag (the genuine v-if/v-else case).
        let source = "    <NButton v-if=\"x\" />\n    <NButton v-if=\"!x\" />";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn ignores_pair_with_intervening_always_rendered_sibling() {
        // Regression #7590 (koel EditUserForm): an always-rendered `<FormRow>`
        // sibling sits at the same indent between the two opposite `v-if`s, so
        // the negated one's directly-preceding sibling is that `<FormRow>`, not
        // the base — `v-else` is impossible.
        let source = "  <AlertBox v-if=\"sso\">info</AlertBox>\n\
            \x20 <FormRow>Name</FormRow>\n\
            \x20 <FormRow v-if=\"!sso\">Password</FormRow>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_pair_with_intervening_v_else_chain() {
        // Regression #7590: the base `v-if` chain is already closed by a
        // `v-else-if` at the same indent between the two `v-if`s, so the second
        // is a fresh element that cannot become the base's `v-else`.
        let source = "  <div v-if=\"x\">A</div>\n\
            \x20 <div v-else-if=\"y\">B</div>\n\
            \x20 <div v-if=\"!x\">C</div>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_negated_element_in_named_slot() {
        // Regression #7590 (koel ChangePasswordForm): the negated `v-if` is on a
        // `<template … #footer>` targeting a distinct named slot, which cannot
        // be the `v-else` of the base element.
        let source = "  <AlertBox v-if=\"sso\">info</AlertBox>\n\
            \x20 <template v-if=\"!sso\" #footer>x</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_leading_not_with_shared_and_operand() {
        // Regression #7693 (jeecgboot BasicSubMenuItem): `!A && B` and `A && B`
        // are mutually exclusive but not exhaustive — both are false when `B` is
        // false. The leading `!` binds only to `A`, so the pair is not a
        // `v-if`/`v-else` negation.
        let source = "  <BasicMenuItem v-if=\"!menuHasChildren(item) && getShowMenu\" />\n\
            \x20 <SubMenu v-if=\"menuHasChildren(item) && getShowMenu\" />";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_leading_not_with_shared_or_operand() {
        // A top-level `||` after the leading `!` is likewise not negated by it.
        let source = "  <X v-if=\"!a || b\" />\n  <Y v-if=\"a || b\" />";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_negation_of_whole_call_with_inner_and() {
        // The `&&` sits inside the call's parens (depth 1), so the leading `!`
        // negates the whole `fn(...)` atom — a genuine negation that must flag.
        let source = "  <X v-if=\"fn(a && b)\" />\n  <Y v-if=\"!fn(a && b)\" />";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_negation_of_parenthesized_expr_with_string_and() {
        // The `&&` lives inside a string literal, so it is not a top-level
        // operator; `!(...)` is the true negation of `(...)` and must flag.
        let source = "  <X v-if=\"(x === 'a && b')\" />\n  <Y v-if=\"!(x === 'a && b')\" />";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_plain_atom_negation() {
        // A genuine atom negation (`!isActive` vs `isActive`) must still flag —
        // the top-level-operator guard must not neuter the rule.
        let source = "  <X v-if=\"!isActive\" />\n  <Y v-if=\"isActive\" />";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_genuinely_adjacent_multiline_opposite_v_ifs() {
        // Regression #7590 true positive (koel PreferencesForm): two directly
        // adjacent same-indent siblings, with only the first element's own
        // content and close tag between them, still flag — a real
        // `v-if`/`v-else` case.
        let source = "  <NButton v-if=\"x\">\n\
            \x20   Save\n\
            \x20 </NButton>\n\
            \x20 <NButton v-if=\"!x\">\n\
            \x20   Edit\n\
            \x20 </NButton>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }
}
