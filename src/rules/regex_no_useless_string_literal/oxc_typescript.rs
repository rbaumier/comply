//! regex-no-useless-string-literal OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::RegExpFlags;
use std::sync::Arc;

pub struct Check;

/// Returns true when `pattern` contains a `\q{a|b|...}` disjunction whose
/// alternatives are all exactly one character long.
fn has_single_char_string_disjunction(pattern: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = pattern[search_from..].find("\\q{") {
        let start = search_from + pos + 3;
        if let Some(end) = pattern[start..].find('}') {
            let content = &pattern[start..start + end];
            let parts: Vec<&str> = content.split('|').collect();
            if parts.len() >= 2 && parts.iter().all(|p| p.chars().count() == 1) {
                return true;
            }
            search_from = start + end + 1;
        } else {
            break;
        }
    }
    false
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

        if !re.regex.flags.contains(RegExpFlags::V) {
            return;
        }
        let pattern = re.regex.pattern.text.as_str();
        if !has_single_char_string_disjunction(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "String disjunction of single characters can be simplified to a character class.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
