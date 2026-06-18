//! regex-no-useless-assertions OxcCheck backend.
//!
//! Visits `RegExpLiteral` nodes only — string literals containing `^` or `$`
//! (URLs, scoped imports, Tailwind values) cannot be mistaken for regex.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::regex_helpers::is_inside_char_class;
use std::sync::Arc;

fn has_useless_dollar(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next != b')' && next != b'|'
                && (i == 0 || bytes[i - 1] != b'\\')
                && !is_inside_char_class(bytes, i) {
                    return true;
                }
        }
    }
    false
}

/// A `^` at index `i` (`i > 0`) begins an alternative — a valid anchor — when it
/// follows a group-open `(`, an alternation `|`, a char-class `[`, an escape `\`,
/// or a group prefix `(?:` / `(?=` / `(?!` / `(?<=` / `(?<!`.
fn caret_begins_alternative(bytes: &[u8], i: usize) -> bool {
    if matches!(bytes[i - 1], b'(' | b'|' | b'[' | b'\\') {
        return true;
    }
    const GROUP_PREFIXES: &[&[u8]] = &[b"(?:", b"(?=", b"(?!", b"(?<=", b"(?<!"];
    GROUP_PREFIXES.iter().any(|p| bytes[..i].ends_with(p))
}

fn has_useless_caret(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'^' && i > 0
            && !caret_begins_alternative(bytes, i)
            && !is_inside_char_class(bytes, i) {
            return true;
        }
    }
    false
}

pub struct Check;

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
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_negated_word_class_boundary_guard() {
        // Issue #103 reproducer.
        let src = r#"const re = /[^\w](?:body|response):\s*z\.(?:object|strictObject)\(/;"#;
        assert!(run_on(src).is_empty(), "[^\\w] is a character class, not an assertion");
    }

    #[test]
    fn allows_dollar_inside_char_class() {
        // `$` inside `[...]` is a literal `$`, not an end-of-line assertion.
        assert!(run_on(r#"const re = /[A-Za-z_$\xA0-￿]/;"#).is_empty());
        assert!(run_on(r#"const re = /[\w$_]+/;"#).is_empty());
    }

    #[test]
    fn allows_caret_inside_char_class() {
        // `^` not at index 1 of a char class is a literal `^`.
        assert!(run_on(r#"const re = /[a^b]/;"#).is_empty());
    }

    #[test]
    fn still_flags_dollar_outside_char_class() {
        assert_eq!(run_on(r#"const re = /[abc]$foo/;"#).len(), 1);
    }

    #[test]
    fn does_not_panic_on_trailing_backslash() {
        // `/\\/` — the pattern seen by the checker is `\\`, trailing backslash
        // after the escape. Must not panic with OOB index.
        assert!(run_on(r#"const re = /\\/;"#).is_empty());
        // Incomplete char class with trailing backslash — also must not panic.
        assert!(run_on(r#"const re = /[\\/;"#).is_empty() || true); // may or may not flag; no panic
    }

    #[test]
    fn allows_dollar_after_literal_close_bracket() {
        // `/[]$]/` — the first `]` after `[` is a literal in JS regex,
        // so `$` here is inside the char class and must not be flagged.
        assert!(run_on(r#"const re = /[]$]/;"#).is_empty());
        // Negated variant: `/[^]$]/`
        assert!(run_on(r#"const re = /[^]$]/;"#).is_empty());
    }

    #[test]
    fn allows_negated_char_class_in_lookahead_lookbehind() {
        // Issue #385: [^\w] inside lookahead/lookbehind must not be flagged.
        let src = r#"const pattern = /(?<=[^\w]|^)keyword(?=[^\w]|$)/;"#;
        assert!(run_on(src).is_empty(), "[^\\w] inside lookahead is a char class, not a useless assertion");
    }

    #[test]
    fn allows_caret_as_first_alternative_of_group() {
        // Issue #3774: `(?:^|,)` means start-of-string OR comma; the `^` is a
        // real anchor. The trailing `(?:,|$)` exercises the `$` valid set too.
        let src = r#"const re = /(?:^|,)\s*?no-transform\s*?(?:,|$)/i;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_caret_after_noncapturing_group_prefix() {
        // Issue #3774 second repro.
        assert!(run_on(r#"const re = /(?:^|\.)__proto__\./;"#).is_empty());
    }

    #[test]
    fn allows_caret_in_lookahead() {
        assert!(run_on(r#"const re = /(?=^foo)/;"#).is_empty());
    }

    #[test]
    fn allows_caret_in_lookbehind() {
        assert!(run_on(r#"const re = /(?<=^)x/;"#).is_empty());
    }

    #[test]
    fn allows_caret_in_negative_lookahead() {
        assert!(run_on(r#"const re = /(?!^)bar/;"#).is_empty());
    }

    #[test]
    fn allows_caret_and_dollar_anchors_in_groups_with_char_classes() {
        // filepath.ts shape: both `^` and `$` begin/end group alternatives.
        assert!(run_on(r#"const re = /(?:^|[\/\\])\.\.(?:$|[\/\\])/;"#).is_empty());
    }
}

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

        if re.regex.flags.contains(oxc_ast::ast::RegExpFlags::M) {
            return;
        }

        let pattern = re.regex.pattern.text.as_str();

        if !has_useless_dollar(pattern) && !has_useless_caret(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion is always true or always false and has no effect.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
