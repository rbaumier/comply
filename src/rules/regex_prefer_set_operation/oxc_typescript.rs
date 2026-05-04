//! regex-prefer-set-operation OXC backend — flag lookaround+char-class patterns
//! that can be rewritten as v-flag set operations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_set_operation_candidate(pattern: &str) -> bool {
    const CANDIDATES: &[&str] = &[
        r"(?=\d)\w",
        r"(?=\w)\d",
        r"(?!\d)\w",
        r"(?!\w)\d",
        r"(?=\s)\w",
        r"(?=\w)\s",
        r"(?!\s)\w",
        r"(?!\w)\s",
    ];
    CANDIDATES.iter().any(|pat| pattern.contains(pat))
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
        let Some(raw) = &re.raw else {
            return;
        };
        let Some(pattern) = extract_pattern(raw.as_str()) else {
            return;
        };
        if !has_set_operation_candidate(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This lookaround + character pattern can be expressed using a v-flag set operation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
