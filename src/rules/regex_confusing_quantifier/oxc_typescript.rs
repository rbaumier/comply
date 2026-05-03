//! OxcCheck backend for regex-confusing-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_confusing_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_optional = false;

            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'?' if depth == 1
                        && j > 0
                        && bytes[j - 1] != b'('
                        && bytes[j - 1] != b'\\' =>
                    {
                        inner_has_optional = true;
                    }
                    b'*' if depth == 1 => {
                        inner_has_optional = true;
                    }
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 && inner_has_optional && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' {
                    return true;
                } else if next == b'{'
                    && let Some(min) = parse_min_quantifier(&pattern[j + 1..])
                    && min > 0
                {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn parse_min_quantifier(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find('}')?;
    let content = &inner[..end];
    let parts: Vec<&str> = content.split(',').collect();
    parts.first()?.parse().ok()
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
        if !has_confusing_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Confusing quantifier \u{2014} minimum is non-zero but the element can match empty string.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
