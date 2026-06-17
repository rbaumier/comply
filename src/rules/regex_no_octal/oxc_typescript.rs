//! regex-no-octal OXC backend.
//!
//! Visits OXC `RegExpLiteral` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_octal_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let group_count = crate::rules::regex_helpers::count_capturing_groups(bytes);
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\\' {
            let mut c = 0;
            let mut j = i;
            while j < len && bytes[j] == b'\\' {
                c += 1;
                j += 1;
            }
            if c % 2 == 1 {
                let after = i + c;
                if after < len
                    && bytes[after].is_ascii_digit()
                    && bytes[after] != b'8'
                    && bytes[after] != b'9'
                {
                    if bytes[after] == b'0' {
                        // `\0` is the octal null escape, never a backreference.
                        if after + 1 < len && bytes[after + 1] >= b'0' && bytes[after + 1] <= b'7'
                        {
                            return true;
                        }
                    } else if !is_backreference(bytes, after, group_count) {
                        return true;
                    }
                }
            }
            i += c;
        } else {
            i += 1;
        }
    }
    false
}

/// Returns true if the digit run starting at `after` (first digit is `1`-`7`)
/// is a backreference `\N` rather than an octal escape, i.e. a capturing group
/// numbered `N` exists. In JS, `\N` is an octal escape only when there is NO
/// capturing group `N`; otherwise it is a valid backreference.
fn is_backreference(bytes: &[u8], after: usize, group_count: usize) -> bool {
    let mut n = 0usize;
    let mut k = after;
    while k < bytes.len() && bytes[k].is_ascii_digit() {
        n = n.saturating_mul(10).saturating_add((bytes[k] - b'0') as usize);
        k += 1;
    }
    n >= 1 && n <= group_count
}

fn extract_pattern(src: &str) -> Option<&str> {
    let src = src.strip_prefix('/')?;
    let bytes = src.as_bytes();
    let mut in_class = false;
    let mut i = 0;
    let mut last_slash = None;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'[' => { in_class = true; i += 1; }
            b']' if in_class => { in_class = false; i += 1; }
            b'/' if !in_class => { last_slash = Some(i); i += 1; }
            _ => i += 1,
        }
    }
    last_slash.map(|pos| &src[..pos])
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
        let AstKind::RegExpLiteral(regex) = node.kind() else { return };

        let src = &ctx.source[regex.span.start as usize..regex.span.end as usize];
        let Some(pattern) = extract_pattern(src) else { return };

        if !has_octal_escape(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Octal escape in regex is ambiguous \u{2014} use a named backreference or Unicode escape instead.".into(),
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

    #[test]
    fn flags_octal_escape_in_regex() {
        assert_eq!(run_on(r#"const re = /\1/;"#).len(), 1);
    }

    #[test]
    fn flags_multi_digit_octal() {
        assert_eq!(run_on(r#"const re = /\12/;"#).len(), 1);
    }

    #[test]
    fn allows_null_escape() {
        assert!(run_on(r#"const re = /\0/;"#).is_empty());
    }

    #[test]
    fn flags_octal_after_null() {
        assert_eq!(run_on(r#"const re = /\00/;"#).len(), 1);
    }

    #[test]
    fn allows_no_regex() {
        assert!(run_on("const x = 42;").is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b\\1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }

    // --- backreference vs octal escape (issue #3925) ---

    #[test]
    fn allows_backreference_with_lookahead_group() {
        // eslint no-mixed-spaces-and-tabs.js:95 — `\1` -> capture group 1.
        let src = r#"const re = /^(?=( +|\t+))\1(?:\t| )/u;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_two_backreferences_with_two_groups() {
        // eslint no-mixed-spaces-and-tabs.js:103 — `\1`/`\2` -> groups 1 and 2.
        let src = r#"const re = /^(?=(\t*))\1(?=( +))\2\t/u;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_backreference_with_capture_group() {
        assert!(run_on(r#"const re = /(x)\1/;"#).is_empty());
    }

    #[test]
    fn allows_backreference_with_named_group() {
        assert!(run_on(r#"const re = /(?<y>x)\1/;"#).is_empty());
    }

    #[test]
    fn flags_octal_with_no_capture_group() {
        // 0 capture groups -> `\1` is octal, still flagged.
        assert_eq!(run_on(r#"const re = /\1/;"#).len(), 1);
    }

    #[test]
    fn flags_octal_when_only_non_capturing_group() {
        // `(?:x)` does not raise the group count, so `\1` is still octal.
        assert_eq!(run_on(r#"const re = /(?:x)\1/;"#).len(), 1);
    }

    #[test]
    fn flags_octal_when_group_number_too_high() {
        // 1 group but `\2` references a group that does not exist -> octal.
        assert_eq!(run_on(r#"const re = /(a)\2/;"#).len(), 1);
    }
}
