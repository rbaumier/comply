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
//! Only consecutive same-indentation opposite `v-if`s are flagged. Pairs with
//! another `v-if` between them (not adjacent siblings) or at different
//! indentation (different nesting depth) are not flagged, because a `v-else`
//! rewrite is then impossible or semantically wrong.
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

        // Collect v-if occurrences in source order, recording indentation so a
        // pair can be checked for adjacency at the same nesting depth.
        let mut occurrences: Vec<VIf> = Vec::new();
        for (idx, line) in masked.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(cond) = extract_v_if_condition(trimmed) {
                occurrences.push(VIf {
                    line: idx + 1,
                    cond: cond.to_string(),
                    indent: leading_whitespace_width(line),
                });
            }
        }

        // Flag only consecutive same-indentation opposite v-ifs: an adjacent
        // sibling pair `v-if="X"` then `v-if="!X"` that a `v-else` could replace.
        let mut diagnostics = Vec::new();
        for pair in occurrences.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if a.indent == b.indent && is_negation_of(&b.cond, &a.cond) {
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
}
