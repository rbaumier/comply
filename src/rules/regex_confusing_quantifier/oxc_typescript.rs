//! OxcCheck backend for regex-confusing-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::regex_helpers::{group_is_nullable, is_inside_char_class};
use std::sync::Arc;

pub struct Check;

/// Flags a group quantified with a non-zero minimum (`+` or `{m,…}` with `m>0`)
/// whose body is *nullable* — it can match the empty string. Such a quantifier
/// claims "at least one" while the element it repeats may consume nothing, which
/// is almost always a bug. A group with any mandatory atom (e.g. `(?:\r?\n)+`,
/// where `\n` always consumes a char) is NOT nullable and is not flagged.
fn has_confusing_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        // `(` inside a `[...]` char class is a literal member, not a group.
        if bytes[i] == b'(' && !is_inside_char_class(bytes, i) {
            let mut depth = 1;
            let mut j = i + 1;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 && j + 1 < len && group_is_nullable(&bytes[i + 1..j]) {
                let next = bytes[j + 1];
                if next == b'+' {
                    return true;
                } else if next == b'{'
                    && let Some(min) = parse_min_quantifier(&pattern[j + 1..])
                    && min > 0
                {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn parse_min_quantifier(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find('}')?;
    let content = &inner[..end];
    let parts: Vec<&str> = content.split(',').collect();
    parts.first()?.parse().ok()
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
        let pattern = re.regex.pattern.text.as_str();
        if !has_confusing_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Confusing quantifier \u{2014} minimum is non-zero but the element can match empty string.".into(),
            severity: Severity::Warning,
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

    // --- True positives: genuinely nullable groups with a min-≥1 quantifier. ---

    #[test]
    fn flags_optional_in_plus_group() {
        assert_eq!(run_on(r#"const re = /(?:a?)+/;"#).len(), 1);
    }

    #[test]
    fn flags_star_in_plus_group() {
        assert_eq!(run_on(r#"const re = /(?:a*)+/;"#).len(), 1);
    }

    #[test]
    fn flags_all_optional_concat_in_plus_group() {
        // Every atom optional → branch nullable → group nullable.
        assert_eq!(run_on(r#"const re = /(?:a?b?)+/;"#).len(), 1);
    }

    #[test]
    fn flags_nullable_alternation_branch() {
        // `a?` branch is nullable → alternation nullable → group nullable.
        assert_eq!(run_on(r#"const re = /(?:a?|b)+/;"#).len(), 1);
    }

    #[test]
    fn flags_only_optional_cr_in_plus_group() {
        // `(?:\r?)+` — the only atom is optional, so the group is nullable.
        assert_eq!(run_on(r#"const re = /(?:\r?)+/;"#).len(), 1);
    }

    #[test]
    fn flags_min_one_brace_quantifier() {
        assert_eq!(run_on(r#"const re = /(?:a?){1,3}/;"#).len(), 1);
    }

    // --- True negatives. ---

    #[test]
    fn allows_required_in_plus_group() {
        assert!(run_on(r#"const re = /(?:a)+/;"#).is_empty());
    }

    #[test]
    fn allows_mandatory_atom_after_optional() {
        // `(?:\r?\n)+` from eslint — `\n` is mandatory, so the group is NOT
        // nullable (issue #3926).
        assert!(run_on(r#"const re = /(?:\r?\n)+$/u;"#).is_empty());
    }

    #[test]
    fn allows_optional_then_mandatory() {
        // `b` is mandatory.
        assert!(run_on(r#"const re = /(?:a?b)+/;"#).is_empty());
    }

    #[test]
    fn allows_mandatory_then_optional() {
        // `a` is mandatory.
        assert!(run_on(r#"const re = /(?:ab?)+/;"#).is_empty());
    }

    #[test]
    fn allows_alternation_with_no_nullable_branch() {
        // Neither `a` nor `b` is nullable → group not nullable.
        assert!(run_on(r#"const re = /(?:a|b)+/;"#).is_empty());
    }

    // --- Regression: char-class context (no `(` group). ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
