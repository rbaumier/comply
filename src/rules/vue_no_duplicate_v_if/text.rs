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

/// Whether `cond` is exactly the boolean negation of `base` (one is the other
/// with a single leading `!`).
fn is_negation_of(cond: &str, base: &str) -> bool {
    cond.strip_prefix('!') == Some(base) || base.strip_prefix('!') == Some(cond)
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
