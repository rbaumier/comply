//! regex-no-slow-pattern OXC backend.
//!
//! Visits `RegExpLiteral` nodes only — string literals that happen to
//! look like regex are never flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::regex_helpers::is_inside_char_class;
use crate::rules::regex_redos::inner_quantifier_on_leading_atom;
use std::sync::Arc;

/// Detects nested quantifiers like `(X+)+`, `(X*)*`, `(X+)*`, etc.
fn has_nested_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_quantifier = false;
            let mut in_character_class = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => {
                        j += 1;
                    }
                    b'[' => in_character_class = true,
                    b']' => in_character_class = false,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'+' | b'*' if !in_character_class => inner_has_quantifier = true,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && inner_has_quantifier && j + 1 < len {
                let next = bytes[j + 1];
                let body = &bytes[i + 1..j];
                let has_outer_quantifier = next == b'+' || next == b'*';
                // Flag only an ambiguous nested quantifier. A top-level
                // alternation `(?:A|B|C)*` is the disjoint-alternation tokenizer
                // idiom, and the "unrolled loop" `(?:(?!</tag>)<[^<]*)*` anchors
                // its inner `*` behind a mandatory disjoint literal — both are
                // linear-time. A real `(X+)+` / `(a*)*` repeats the group's
                // leading atom with no top-level `|`.
                let is_ambiguous = !body_has_top_level_alternation(body)
                    && inner_quantifier_on_leading_atom(body);
                if has_outer_quantifier && is_ambiguous {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// Returns true if the group `body` (bytes between the group's `(` and its
/// matching `)`) contains an alternation `|` at the body's own nesting level —
/// i.e. a `|` that is neither inside a nested `(...)` group nor inside a `[...]`
/// character class.
fn body_has_top_level_alternation(body: &[u8]) -> bool {
    let mut depth = 0u32;
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'(' if !is_inside_char_class(body, i) => depth += 1,
            b')' if depth > 0 && !is_inside_char_class(body, i) => depth -= 1,
            b'|' if depth == 0 && !is_inside_char_class(body, i) => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !has_nested_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested quantifier detected \u{2014} this pattern can cause catastrophic backtracking (ReDoS).".into(),
            severity: Severity::Error,
            span: None,
        });
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

    // --- True positives: genuine nested quantifier, no top-level alternation. ---

    #[test]
    fn flags_plus_plus() {
        assert_eq!(run_on(r#"const re = /(a+)+$/;"#).len(), 1);
    }

    #[test]
    fn flags_star_star() {
        assert_eq!(run_on(r#"const re = /(a*)*/;"#).len(), 1);
    }

    #[test]
    fn flags_digit_plus_plus() {
        assert_eq!(run_on(r#"const re = /(\d+)+/;"#).len(), 1);
    }

    #[test]
    fn flags_char_class_plus_star() {
        assert_eq!(run_on(r#"const re = /([a-z]+)*/;"#).len(), 1);
    }

    #[test]
    fn flags_dotstar_star() {
        assert_eq!(run_on(r#"const re = /(.*)*$/;"#).len(), 1);
    }

    #[test]
    fn flags_alternation_inside_nested_group() {
        // The `|` is inside the NESTED group `(a|b)`; the OUTER group's top level
        // is `(a|b)+`, which has no top-level `|`, so this still flags.
        assert_eq!(run_on(r#"const re = /((a|b)+)+/;"#).len(), 1);
    }

    // --- Negatives: a top-level alternation under the outer quantifier. ---

    #[test]
    fn allows_babel_skip_whitespace() {
        // babel whitespace.ts:31 — disjoint `(?:A|B|C)*` skipper, linear-time.
        assert!(run_on(r#"const re = /(?:\s|\/\/.*|\/\*[^]*?\*\/)*/g;"#).is_empty());
    }

    #[test]
    fn allows_babel_skip_whitespace_in_line() {
        // babel whitespace.ts:34 — disjoint `(?:A|B|C)*` skipper, linear-time.
        assert!(run_on(r#"const re = /(?:[^\S\n\r ]|\/\/.*|\/\*.*?\*\/)*/g;"#).is_empty());
    }

    #[test]
    fn allows_two_branch_alternation() {
        // Top-level `|` under the outer `*` → conservative, no warning.
        assert!(run_on(r#"const re = /(?:a*|b*)*/;"#).is_empty());
    }

    #[test]
    fn allows_unrolled_loop_tag_stripper() {
        // Kuingsmile/word-GPT-Plus generalTools.ts:35-36 — Friedl's "unrolling
        // the loop" tag stripper. The literal `<` (disjoint from the repeated
        // `[^<]` class) anchors every iteration → linear-time, not catastrophic.
        assert!(
            run_on(r#"const re = /<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi;"#).is_empty()
        );
        assert!(
            run_on(r#"const re = /<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>/gi;"#).is_empty()
        );
    }

    // --- Existing baseline: single quantifier / no quantified group. ---

    #[test]
    fn allows_single_quantifier() {
        assert!(run_on(r#"const re = /(a+)/;"#).is_empty());
    }

    #[test]
    fn allows_non_quantified_group() {
        assert!(run_on(r#"const re = /(abc)/;"#).is_empty());
    }

    #[test]
    fn ignores_plus_literal_in_character_class() {
        assert!(run_on(r#"const re = /([a+])+/;"#).is_empty());
    }
}
