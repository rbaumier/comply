use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Count `\\` pairs in a string node's source text.
fn count_escaped_backslashes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // The rule only fires on a literal carrying two escaped backslashes,
        // so the source must contain at least one `\\` byte pair.
        Some(&["\\\\"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else { return };
        let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];

        // Skip strings containing backticks (can't use String.raw with backticks)
        if raw.contains('`') {
            return;
        }

        // Skip strings with interpolation patterns
        if raw.contains("${") {
            return;
        }

        // `String.raw` cannot represent a value ending in a backslash — the
        // trailing `\` escapes the closing backtick (`String.raw`\`` is a
        // syntax error). Guard against malformed spans (oxc edge case on
        // very large repos) where raw.len() < 2 would make the slice panic.
        if raw.len() < 2 {
            return;
        }
        if raw[1..raw.len() - 1].ends_with('\\') {
            return;
        }

        if count_escaped_backslashes(raw) >= 2 {
            let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`String.raw` should be used to avoid escaping `\\`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_multi_backslash_string() {
        // `"\d\w+"` — two escaped backslashes, String.raw is a readability win.
        assert_eq!(run_on(r#"const re = "\\d\\w+";"#).len(), 1);
    }

    // Regression for #234: a single-backslash LIKE-escape constant needs no
    // String.raw, and String.raw can't carry a trailing backslash anyway.
    #[test]
    fn ignores_single_backslash() {
        assert!(run_on(r#"const LIKE_ESCAPE = "\\";"#).is_empty());
    }

    // Regression for #234: a value ending in a backslash cannot be a String.raw
    // literal even with 2+ escapes.
    #[test]
    fn ignores_string_ending_in_backslash() {
        assert!(run_on(r#"const x = "\\d\\";"#).is_empty());
    }

    // Regression for #777: empty string and single-char strings must not panic
    // (oxc can emit spans of length 0 or 1 on very large repos like microsoft/TypeScript).
    #[test]
    fn no_panic_on_short_string() {
        assert!(run_on(r#"const a = "";"#).is_empty());
        assert!(run_on(r#"const b = "x";"#).is_empty());
    }
}
