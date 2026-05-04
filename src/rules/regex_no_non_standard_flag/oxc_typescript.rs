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
