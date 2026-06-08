//! regex-sort-flags OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Extract the flags string from the raw source text of a regex literal.
fn flags_from_source(source: &str, span_start: usize, span_end: usize) -> &str {
    let text = &source[span_start..span_end];
    // Regex literal: /pattern/flags — find last `/`.
    if let Some(last_slash) = text.rfind('/') {
        &text[last_slash + 1..]
    } else {
        ""
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

        let flags = flags_from_source(ctx.source, re.span.start as usize, re.span.end as usize);
        if flags.len() < 2 {
            return;
        }
        let mut sorted: Vec<u8> = flags.bytes().collect();
        sorted.sort_unstable();
        if flags.as_bytes() == sorted.as_slice() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Regex flags are not sorted alphabetically \u{2014} reorder them (e.g. `dgimsvy`).".into(),
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
    fn flags_unsorted_gi() {
        assert_eq!(run_on(r#"const re = /foo/ig;"#).len(), 1);
    }


    #[test]
    fn flags_unsorted_mig() {
        assert_eq!(run_on(r#"const re = /bar/mig;"#).len(), 1);
    }


    #[test]
    fn allows_sorted_flags() {
        assert!(run_on(r#"const re = /foo/gi;"#).is_empty());
    }


    #[test]
    fn allows_single_flag() {
        assert!(run_on(r#"const re = /foo/g;"#).is_empty());
    }


    #[test]
    fn allows_no_flags() {
        assert!(run_on(r#"const re = /foo/;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
