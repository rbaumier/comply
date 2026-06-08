//! regex-no-unused-groups — OXC backend.
//! Visits `RegExpLiteral` nodes and flags named capturing groups that are
//! never referenced elsewhere in the file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Extract named capturing group names from `pattern`.
/// Skips lookbehind constructs `(?<=...)` and `(?<!...)`.
fn extract_named_groups(pattern: &str) -> Vec<(String, usize)> {
    let mut groups = Vec::new();
    let bytes = pattern.as_bytes();
    let needle = b"(?<";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }
        let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
        if backslashes % 2 != 0 {
            i += 1;
            continue;
        }
        let name_start = i + needle.len();
        if name_start < bytes.len() && (bytes[name_start] == b'=' || bytes[name_start] == b'!') {
            i = name_start;
            continue;
        }
        if let Some(rel_end) = pattern[name_start..].find('>') {
            let name = &pattern[name_start..name_start + rel_end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                groups.push((name.to_string(), i));
            }
            i = name_start + rel_end + 1;
        } else {
            break;
        }
    }
    groups
}

/// Checks whether a named group `name` is referenced somewhere in the file.
fn is_group_referenced(name: &str, source: &str) -> bool {
    let dot_access = format!(".groups.{name}");
    let bracket_access_dq = format!("groups[\"{name}\"]");
    let bracket_access_sq = format!("groups['{name}']");
    let replacement_ref = format!("$<{name}>");
    crate::oxc_helpers::source_contains(source, &dot_access)
        || crate::oxc_helpers::source_contains(source, &bracket_access_dq)
        || crate::oxc_helpers::source_contains(source, &bracket_access_sq)
        || crate::oxc_helpers::source_contains(source, &replacement_ref)
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
        let groups = extract_named_groups(pattern);
        if groups.is_empty() {
            return;
        }
        for (name, _offset) in groups {
            if is_group_referenced(&name, ctx.source) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Named capturing group `{name}` is never referenced \u{2014} use `.groups.{name}` or convert to `(?:...)`."
                ),
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
    fn flags_unused_named_group() {
        let src = r#"const re = /(?<year>\d{4})-(?<month>\d{2})/;"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_used_named_group_dot() {
        let src = "const re = /(?<year>\\d{4})/;\nconst y = match.groups.year;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_replacement() {
        let src = r#"const re = /(?<day>\d{2})/; str.replace(re, "$<day>");"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_lookbehind() {
        let src = r#"const re = /(?<=foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_negative_lookbehind() {
        let src = r#"const re = /(?<!foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_named_group_lookalike_in_string() {
        // OXC only visits RegExpLiteral nodes, so strings are never flagged.
        let src = r#"const x = "grid-[(?<foo>_auto)]";"#;
        assert!(run_on(src).is_empty());
    }
}
