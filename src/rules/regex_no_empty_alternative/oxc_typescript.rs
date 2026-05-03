//! OXC backend for regex-no-empty-alternative.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn pattern_has_empty_alternative(pattern: &str) -> bool {
    pattern.starts_with('|') || pattern.ends_with('|') || pattern.contains("||")
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

        if !pattern_has_empty_alternative(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty alternative in regex \u{2014} remove leading, trailing, or consecutive `|`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
