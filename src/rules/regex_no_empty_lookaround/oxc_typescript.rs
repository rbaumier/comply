//! regex-no-empty-lookaround OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const EMPTY_LOOKAROUNDS: &[&str] = &["(?=)", "(?!)", "(?<=)", "(?<!)"];

fn has_empty_lookaround(pattern: &str) -> bool {
    EMPTY_LOOKAROUNDS.iter().any(|n| pattern.contains(n))
}

/// Extract the pattern from a regex literal's `raw` field (e.g. `/foo/g` -> `foo`).
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

        if !has_empty_lookaround(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty lookaround always matches or always fails \u{2014} add a pattern or remove it.".into(),
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
    fn flags_empty_lookahead() {
        assert_eq!(run_on("const re = /foo(?=)/;").len(), 1);
    }


    #[test]
    fn flags_empty_negative_lookahead() {
        assert_eq!(run_on("const re = /foo(?!)/;").len(), 1);
    }


    #[test]
    fn flags_empty_lookbehind() {
        assert_eq!(run_on("const re = /(?<=)bar/;").len(), 1);
    }


    #[test]
    fn flags_empty_negative_lookbehind() {
        assert_eq!(run_on("const re = /(?<!)bar/;").len(), 1);
    }


    #[test]
    fn allows_non_empty_lookahead() {
        assert!(run_on("const re = /foo(?=bar)/;").is_empty());
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
