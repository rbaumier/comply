//! regex-no-dupe-disjunctions OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_dupe_alternatives(pattern: &str) -> bool {
    let alts = split_top_level_alternatives(pattern);
    if alts.len() < 2 {
        return false;
    }
    for i in 0..alts.len() {
        for j in (i + 1)..alts.len() {
            if alts[i] == alts[j] && !alts[i].is_empty() {
                return true;
            }
        }
    }
    false
}

fn split_top_level_alternatives(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'|' if depth == 0 => {
                alts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    alts.push(&pattern[start..]);
    alts
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let Some(raw) = &re.raw else { return };
        let Some(pattern) = extract_pattern(raw.as_str()) else { return };

        if !has_dupe_alternatives(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Duplicate alternative in regex disjunction \u{2014} remove the redundant branch.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_alternative() {
        assert_eq!(run_on(r#"const re = /foo|bar|foo/;"#).len(), 1);
    }

    #[test]
    fn allows_unique_alternatives() {
        assert!(run_on(r#"const re = /foo|bar|baz/;"#).is_empty());
    }

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
