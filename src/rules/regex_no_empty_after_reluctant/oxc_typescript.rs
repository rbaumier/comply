use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// A reluctant quantifier (`*?`, `+?`, `??`) is useless when nothing
/// consumable can follow it, because it then always matches the minimum.
/// That is the case when it sits directly before the end of the pattern,
/// before the `$` end anchor, or at the tail of one or more groups whose
/// closes lead straight to the end of the pattern / `$`. When real pattern
/// follows (e.g. `(.*?)\*\/`), reluctance does work — match up to the first
/// occurrence — so it is not flagged.
fn has_useless_reluctant(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let n = bytes.len();
    if n < 2 {
        return false;
    }
    for i in 0..n {
        let q = bytes[i];
        if (q == b'*' || q == b'+' || q == b'?')
            && i + 1 < n
            && bytes[i + 1] == b'?'
            && (i > 0 && bytes[i - 1] != b'\\')
        {
            let after_idx = i + 2;
            if after_idx >= n {
                return true;
            }
            let next = bytes[after_idx];
            if next == b'$' {
                return true; // directly before the end anchor — reluctance is useless
            }
            if next == b')' {
                // At the tail of its group. Skip consecutive group-closes; only useless
                // if nothing consumable follows (end-of-pattern or the `$` end anchor).
                // If real pattern follows the group (e.g. `(.*?)\*\/`), the reluctance does
                // work — match up to the FIRST occurrence — so it is NOT useless.
                let mut j = after_idx;
                while j < n && bytes[j] == b')' {
                    j += 1;
                }
                if j >= n || bytes[j] == b'$' {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract the pattern from a regex literal's `raw` field (e.g. `/foo|bar/g` -> `foo|bar`).
fn extract_pattern(raw: &str) -> Option<&str> {
    let s = raw.strip_prefix('/')?;
    let last_slash = s.rfind('/')?;
    Some(&s[..last_slash])
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };

        let Some(raw) = &re.raw else { return };
        let Some(pattern) = extract_pattern(raw.as_str()) else {
            return;
        };

        if !has_useless_reluctant(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Reluctant quantifier before end-of-pattern is useless \u{2014} it always matches the minimum."
                    .into(),
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

    // --- FP fixed: reluctant group followed by more consumable pattern. ---

    #[test]
    fn allows_reluctant_group_before_more_pattern_comment_body() {
        assert!(run_on(r#"const re = /^\s*\/\*(.*?)\*\//;"#).is_empty());
    }

    #[test]
    fn allows_reluctant_group_before_literal_delimiter() {
        assert!(run_on(r###"const re = /"##(.+?)##"/g;"###).is_empty());
    }

    #[test]
    fn allows_reluctant_group_before_single_char() {
        assert!(run_on("const re = /(.*?)x/;").is_empty());
    }

    // --- True positives preserved. ---

    #[test]
    fn flags_reluctant_group_before_dollar() {
        assert_eq!(run_on(r#"const re = /\.([a-zA-Z0-9]+?)$/;"#).len(), 1);
    }

    #[test]
    fn flags_reluctant_star_group_before_dollar() {
        assert_eq!(run_on("const re = /(.*?)$/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_star_before_dollar_no_group() {
        assert_eq!(run_on("const re = /.*?$/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_star_at_end_no_group() {
        assert_eq!(run_on("const re = /a.*?/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_through_nested_closes_then_end() {
        assert_eq!(run_on("const re = /((.+?))/;").len(), 1);
    }
}
