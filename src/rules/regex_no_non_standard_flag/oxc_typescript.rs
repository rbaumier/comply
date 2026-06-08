//! regex-no-non-standard-flag oxc backend — flag regex literals with non-standard flags.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const STANDARD_FLAGS: &[u8] = b"dgimsuvy";

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

        let _flags = re.regex.flags;
        // Get the raw text to check character-by-character for non-standard flags.
        // The typed `RegExpFlags` only has known flags; we need the raw source to
        // detect unknown characters.
        let raw = &ctx.source[re.span.start as usize..re.span.end as usize];
        let Some(last_slash) = raw.rfind('/') else { return };
        let flags_str = &raw[last_slash + 1..];
        if flags_str.is_empty() {
            return;
        }
        if flags_str.bytes().all(|f| STANDARD_FLAGS.contains(&f)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Non-standard regex flag detected \u{2014} standard flags are: d, g, i, m, s, u, v, y.".into(),
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
    fn allows_standard_flags() {
        assert!(run_on(r#"const re = /foo/gim;"#).is_empty());
    }


    #[test]
    fn allows_no_flags() {
        assert!(run_on(r#"const re = /foo/;"#).is_empty());
    }


    #[test]
    fn ignores_url_with_y_segment() {
        // /query was flagged as `q` flag under the text-based impl.
        let src = r#"const u = "http://localhost:6762/api/v1/diffs/query";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_import_path_with_y() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }
}
